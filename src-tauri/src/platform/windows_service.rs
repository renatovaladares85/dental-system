use std::sync::mpsc::{Receiver, SyncSender};

#[cfg(any(windows, test))]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[cfg(not(windows))]
use std::sync::Arc;

#[cfg(any(windows, test))]
use super::PlatformError;
use super::PlatformResult;

pub const SERVICE_NAME: &str = "OfflineDentalSystem";
#[cfg(any(windows, test))]
pub const SERVICE_READINESS_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(any(windows, test))]
pub const SERVICE_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(10);

#[cfg(any(windows, test))]
const SERVICE_STATUS_UPDATE_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServiceLifecycleState {
    StartPending { checkpoint: u32 },
    Running,
    Stopped { successful: bool },
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy)]
struct ServiceSupervisorTiming {
    readiness_timeout: Duration,
    shutdown_grace_period: Duration,
    status_update_interval: Duration,
}

#[cfg(any(windows, test))]
impl ServiceSupervisorTiming {
    const PRODUCTION: Self = Self {
        readiness_timeout: SERVICE_READINESS_TIMEOUT,
        shutdown_grace_period: SERVICE_SHUTDOWN_GRACE_PERIOD,
        status_update_interval: SERVICE_STATUS_UPDATE_INTERVAL,
    };
}

#[cfg(any(windows, test))]
fn request_shutdown(shutdown: &SyncSender<()>, requested: &AtomicBool) {
    if !requested.swap(true, Ordering::AcqRel) {
        let _ = shutdown.try_send(());
    }
}

#[cfg(any(windows, test))]
fn supervise_service_host(
    readiness: Receiver<()>,
    results: Receiver<PlatformResult<()>>,
    shutdown: &SyncSender<()>,
    shutdown_requested: &AtomicBool,
    timing: ServiceSupervisorTiming,
    mut publish: impl FnMut(ServiceLifecycleState) -> PlatformResult<()>,
) -> PlatformResult<()> {
    let mut checkpoint = 1;
    publish(ServiceLifecycleState::StartPending { checkpoint })?;
    let readiness_deadline = Instant::now() + timing.readiness_timeout;

    loop {
        if let Ok(result) = results.try_recv() {
            let result = result.and(Err(PlatformError::new("SERVICE_HOST_STOPPED")));
            publish(ServiceLifecycleState::Stopped {
                successful: result.is_ok(),
            })?;
            return result;
        }

        if shutdown_requested.load(Ordering::Acquire) {
            return finish_after_shutdown(
                &results,
                shutdown,
                shutdown_requested,
                timing,
                &mut publish,
            );
        }

        let remaining = readiness_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            request_shutdown(shutdown, shutdown_requested);
            let _ = results.recv_timeout(timing.shutdown_grace_period);
            let result = Err(PlatformError::new("SERVICE_READINESS_TIMEOUT"));
            publish(ServiceLifecycleState::Stopped { successful: false })?;
            return result;
        }

        match readiness.recv_timeout(remaining.min(timing.status_update_interval)) {
            Ok(()) => {
                publish(ServiceLifecycleState::Running)?;
                return wait_for_host_after_readiness(
                    &results,
                    shutdown,
                    shutdown_requested,
                    timing,
                    &mut publish,
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                checkpoint += 1;
                publish(ServiceLifecycleState::StartPending { checkpoint })?;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                request_shutdown(shutdown, shutdown_requested);
                let _ = results.recv_timeout(timing.shutdown_grace_period);
                let result = Err(PlatformError::new("SERVICE_READINESS_FAILED"));
                publish(ServiceLifecycleState::Stopped { successful: false })?;
                return result;
            }
        }
    }
}

#[cfg(any(windows, test))]
fn wait_for_host_after_readiness(
    results: &Receiver<PlatformResult<()>>,
    shutdown: &SyncSender<()>,
    shutdown_requested: &AtomicBool,
    timing: ServiceSupervisorTiming,
    publish: &mut impl FnMut(ServiceLifecycleState) -> PlatformResult<()>,
) -> PlatformResult<()> {
    while !shutdown_requested.load(Ordering::Acquire) {
        match results.recv_timeout(timing.status_update_interval) {
            Ok(result) => {
                publish(ServiceLifecycleState::Stopped {
                    successful: result.is_ok(),
                })?;
                return result;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let result = Err(PlatformError::new("SERVICE_HOST_THREAD_FAILED"));
                publish(ServiceLifecycleState::Stopped { successful: false })?;
                return result;
            }
        }
    }

    finish_after_shutdown(results, shutdown, shutdown_requested, timing, publish)
}

#[cfg(any(windows, test))]
fn finish_after_shutdown(
    results: &Receiver<PlatformResult<()>>,
    shutdown: &SyncSender<()>,
    shutdown_requested: &AtomicBool,
    timing: ServiceSupervisorTiming,
    publish: &mut impl FnMut(ServiceLifecycleState) -> PlatformResult<()>,
) -> PlatformResult<()> {
    request_shutdown(shutdown, shutdown_requested);
    let result = results
        .recv_timeout(timing.shutdown_grace_period)
        .unwrap_or_else(|_| Err(PlatformError::new("SERVICE_HOST_STOP_TIMEOUT")));
    publish(ServiceLifecycleState::Stopped {
        successful: result.is_ok(),
    })?;
    result
}

/// Adapter implemented by the HTTP host. `run` must block until shutdown is
/// requested and all listeners/workers have stopped.
pub trait ServiceHost: Send + Sync + 'static {
    fn run(&self, readiness: SyncSender<()>, shutdown: Receiver<()>) -> PlatformResult<()>;
}

impl<F> ServiceHost for F
where
    F: Fn(SyncSender<()>, Receiver<()>) -> PlatformResult<()> + Send + Sync + 'static,
{
    fn run(&self, readiness: SyncSender<()>, shutdown: Receiver<()>) -> PlatformResult<()> {
        self(readiness, shutdown)
    }
}

#[cfg(windows)]
mod windows {
    use std::{
        ffi::OsString,
        sync::{Arc, OnceLock, atomic::AtomicBool, mpsc},
        time::Duration,
    };

    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
        service_dispatcher,
    };

    use super::{
        SERVICE_NAME, ServiceHost, ServiceLifecycleState, ServiceSupervisorTiming,
        request_shutdown, supervise_service_host,
    };
    use crate::platform::{PlatformError, PlatformResult};

    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
    static HOST: OnceLock<Arc<dyn ServiceHost>> = OnceLock::new();

    define_windows_service!(ffi_service_main, service_main);

    pub fn run_dispatcher(host: Arc<dyn ServiceHost>) -> PlatformResult<()> {
        HOST.set(host).map_err(|_| PlatformError::invalid_state())?;
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .map_err(|_| PlatformError::unavailable())
    }

    fn service_main(_arguments: Vec<OsString>) {
        let _ = run_service();
    }

    fn run_service() -> PlatformResult<()> {
        let host = HOST.get().ok_or_else(PlatformError::invalid_state)?.clone();
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let status_slot: Arc<OnceLock<ServiceStatusHandle>> = Arc::new(OnceLock::new());
        let handler_status = status_slot.clone();
        let handler_shutdown = shutdown_tx.clone();
        let handler_shutdown_requested = shutdown_requested.clone();
        let event_handler = move |event| match event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop | ServiceControl::Shutdown => {
                if let Some(status) = handler_status.get() {
                    let _ = status.set_service_status(ServiceStatus {
                        service_type: SERVICE_TYPE,
                        current_state: ServiceState::StopPending,
                        controls_accepted: ServiceControlAccept::empty(),
                        exit_code: ServiceExitCode::Win32(0),
                        checkpoint: 1,
                        wait_hint: Duration::from_secs(20),
                        process_id: None,
                    });
                }
                request_shutdown(&handler_shutdown, &handler_shutdown_requested);
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        };
        let status = service_control_handler::register(SERVICE_NAME, event_handler)
            .map_err(|_| PlatformError::unavailable())?;
        status_slot
            .set(status)
            .map_err(|_| PlatformError::invalid_state())?;

        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ods-service-host".to_owned())
            .spawn(move || {
                let _ = result_tx.send(host.run(ready_tx, shutdown_rx));
            })
            .map_err(|_| PlatformError::new("SERVICE_HOST_THREAD_FAILED"))?;

        supervise_service_host(
            ready_rx,
            result_rx,
            &shutdown_tx,
            &shutdown_requested,
            ServiceSupervisorTiming::PRODUCTION,
            |state| {
                let (current_state, controls_accepted, exit_code, checkpoint, wait_hint) =
                    match state {
                        ServiceLifecycleState::StartPending { checkpoint } => (
                            ServiceState::StartPending,
                            ServiceControlAccept::empty(),
                            ServiceExitCode::Win32(0),
                            checkpoint,
                            Duration::from_secs(20),
                        ),
                        ServiceLifecycleState::Running => (
                            ServiceState::Running,
                            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
                            ServiceExitCode::Win32(0),
                            0,
                            Duration::ZERO,
                        ),
                        ServiceLifecycleState::Stopped { successful } => (
                            ServiceState::Stopped,
                            ServiceControlAccept::empty(),
                            if successful {
                                ServiceExitCode::Win32(0)
                            } else {
                                ServiceExitCode::ServiceSpecific(1)
                            },
                            0,
                            Duration::ZERO,
                        ),
                    };
                status
                    .set_service_status(ServiceStatus {
                        service_type: SERVICE_TYPE,
                        current_state,
                        controls_accepted,
                        exit_code,
                        checkpoint,
                        wait_hint,
                        process_id: None,
                    })
                    .map_err(|_| PlatformError::unavailable())
            },
        )
    }
}

#[cfg(windows)]
pub use windows::run_dispatcher;

#[cfg(not(windows))]
pub fn run_dispatcher(_host: Arc<dyn ServiceHost>) -> PlatformResult<()> {
    Err(PlatformError::unavailable())
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };

    use super::{
        PlatformError, PlatformResult, ServiceLifecycleState, ServiceSupervisorTiming,
        request_shutdown, supervise_service_host,
    };

    fn timing() -> ServiceSupervisorTiming {
        ServiceSupervisorTiming {
            readiness_timeout: Duration::from_millis(30),
            shutdown_grace_period: Duration::from_millis(30),
            status_update_interval: Duration::from_millis(5),
        }
    }

    fn supervise_with(
        readiness: mpsc::Receiver<()>,
        results: mpsc::Receiver<PlatformResult<()>>,
        shutdown: &mpsc::SyncSender<()>,
        shutdown_requested: &AtomicBool,
        publish: impl FnMut(ServiceLifecycleState) -> PlatformResult<()>,
    ) -> PlatformResult<()> {
        supervise_service_host(
            readiness,
            results,
            shutdown,
            shutdown_requested,
            timing(),
            publish,
        )
    }

    #[test]
    fn host_readiness_publishes_running_only_after_start_pending() {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let (shutdown_tx, _shutdown_rx) = mpsc::sync_channel(1);
        let shutdown_requested = AtomicBool::new(false);
        let states = Arc::new(Mutex::new(Vec::new()));
        ready_tx.send(()).expect("send readiness");

        let observed_states = states.clone();
        let result = supervise_with(
            ready_rx,
            result_rx,
            &shutdown_tx,
            &shutdown_requested,
            move |state| {
                observed_states.lock().expect("states").push(state);
                if state == ServiceLifecycleState::Running {
                    result_tx.send(Ok(())).expect("finish host");
                }
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert_eq!(
            *states.lock().expect("states"),
            vec![
                ServiceLifecycleState::StartPending { checkpoint: 1 },
                ServiceLifecycleState::Running,
                ServiceLifecycleState::Stopped { successful: true },
            ]
        );
    }

    #[test]
    fn host_failure_before_readiness_stops_with_the_original_code() {
        let (_ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let (shutdown_tx, _shutdown_rx) = mpsc::sync_channel(1);
        let shutdown_requested = AtomicBool::new(false);
        result_tx
            .send(Err(PlatformError::new("STARTUP_DATABASE_FAILED")))
            .expect("fail host");
        let states = Arc::new(Mutex::new(Vec::new()));
        let observed_states = states.clone();

        let error = supervise_with(
            ready_rx,
            result_rx,
            &shutdown_tx,
            &shutdown_requested,
            move |state| {
                observed_states.lock().expect("states").push(state);
                Ok(())
            },
        )
        .expect_err("startup failure");

        assert_eq!(error.code(), "STARTUP_DATABASE_FAILED");
        assert_eq!(
            states.lock().expect("states").last(),
            Some(&ServiceLifecycleState::Stopped { successful: false })
        );
    }

    #[test]
    fn closed_readiness_channel_requests_shutdown_and_stops() {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (_result_tx, result_rx) = mpsc::sync_channel(1);
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let shutdown_requested = AtomicBool::new(false);
        drop(ready_tx);

        let error = supervise_with(
            ready_rx,
            result_rx,
            &shutdown_tx,
            &shutdown_requested,
            |_| Ok(()),
        )
        .expect_err("readiness channel failure");

        assert_eq!(error.code(), "SERVICE_READINESS_FAILED");
        assert!(shutdown_rx.recv_timeout(Duration::from_millis(5)).is_ok());
    }

    #[test]
    fn readiness_timeout_is_deterministic_when_host_never_signals() {
        let (_ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (_result_tx, result_rx) = mpsc::sync_channel(1);
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let shutdown_requested = AtomicBool::new(false);
        let started = Instant::now();

        let error = supervise_with(
            ready_rx,
            result_rx,
            &shutdown_tx,
            &shutdown_requested,
            |_| Ok(()),
        )
        .expect_err("readiness timeout");

        assert_eq!(error.code(), "SERVICE_READINESS_TIMEOUT");
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(shutdown_rx.recv_timeout(Duration::from_millis(5)).is_ok());
    }

    #[test]
    fn checkpoints_advance_while_service_is_start_pending() {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let (shutdown_tx, _shutdown_rx) = mpsc::sync_channel(1);
        let shutdown_requested = AtomicBool::new(false);
        let checkpoints = Arc::new(Mutex::new(Vec::new()));
        let observed_checkpoints = checkpoints.clone();

        let result = supervise_with(
            ready_rx,
            result_rx,
            &shutdown_tx,
            &shutdown_requested,
            move |state| {
                if let ServiceLifecycleState::StartPending { checkpoint } = state {
                    observed_checkpoints
                        .lock()
                        .expect("checkpoints")
                        .push(checkpoint);
                    if checkpoint == 2 {
                        ready_tx.send(()).expect("send readiness");
                    }
                }
                if state == ServiceLifecycleState::Running {
                    result_tx.send(Ok(())).expect("finish host");
                }
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert_eq!(*checkpoints.lock().expect("checkpoints"), vec![1, 2]);
    }

    #[test]
    fn stop_request_callback_is_non_blocking_and_sends_shutdown_once() {
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        let shutdown_requested = AtomicBool::new(false);
        let started = Instant::now();

        request_shutdown(&shutdown_tx, &shutdown_requested);
        request_shutdown(&shutdown_tx, &shutdown_requested);

        assert!(started.elapsed() < Duration::from_millis(5));
        assert!(shutdown_requested.load(Ordering::Acquire));
        assert!(shutdown_rx.recv_timeout(Duration::from_millis(5)).is_ok());
        assert!(shutdown_rx.try_recv().is_err());
    }

    #[test]
    fn host_result_after_readiness_controls_the_final_stopped_state() {
        for (host_result, expected_success) in [
            (Ok(()), true),
            (Err(PlatformError::new("SERVICE_HOST_THREAD_FAILED")), false),
        ] {
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let (result_tx, result_rx) = mpsc::sync_channel(1);
            let (shutdown_tx, _shutdown_rx) = mpsc::sync_channel(1);
            let shutdown_requested = AtomicBool::new(false);
            let states = Arc::new(Mutex::new(Vec::new()));
            ready_tx.send(()).expect("send readiness");
            let observed_states = states.clone();

            let result = supervise_with(
                ready_rx,
                result_rx,
                &shutdown_tx,
                &shutdown_requested,
                move |state| {
                    observed_states.lock().expect("states").push(state);
                    if state == ServiceLifecycleState::Running {
                        result_tx.send(host_result).expect("finish host");
                    }
                    Ok(())
                },
            );

            assert_eq!(result.is_ok(), expected_success);
            assert_eq!(
                states.lock().expect("states").last(),
                Some(&ServiceLifecycleState::Stopped {
                    successful: expected_success,
                })
            );
        }
    }

    #[test]
    fn stop_request_waits_only_for_the_shutdown_grace_period() {
        let (_ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (_result_tx, result_rx) = mpsc::sync_channel(1);
        let (shutdown_tx, _shutdown_rx) = mpsc::sync_channel(1);
        let shutdown_requested = AtomicBool::new(false);
        request_shutdown(&shutdown_tx, &shutdown_requested);

        let error = supervise_with(
            ready_rx,
            result_rx,
            &shutdown_tx,
            &shutdown_requested,
            |_| Ok(()),
        )
        .expect_err("host ignored stop");

        assert_eq!(error.code(), "SERVICE_HOST_STOP_TIMEOUT");
    }
}
