use std::sync::mpsc::{Receiver, SyncSender};

#[cfg(not(windows))]
use std::sync::Arc;

#[cfg(not(windows))]
use super::PlatformError;
use super::PlatformResult;

pub const SERVICE_NAME: &str = "OfflineDentalSystem";

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
        sync::{Arc, OnceLock, mpsc},
        time::{Duration, Instant},
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

    use super::{SERVICE_NAME, ServiceHost};
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
        let status_slot: Arc<OnceLock<ServiceStatusHandle>> = Arc::new(OnceLock::new());
        let handler_status = status_slot.clone();
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
                let _ = shutdown_tx.try_send(());
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        };
        let status = service_control_handler::register(SERVICE_NAME, event_handler)
            .map_err(|_| PlatformError::unavailable())?;
        status_slot
            .set(status)
            .map_err(|_| PlatformError::invalid_state())?;

        status
            .set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: ServiceState::StartPending,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 1,
                wait_hint: Duration::from_secs(20),
                process_id: None,
            })
            .map_err(|_| PlatformError::unavailable())?;

        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ods-service-host".to_owned())
            .spawn(move || {
                let _ = result_tx.send(host.run(ready_tx, shutdown_rx));
            })
            .map_err(|_| PlatformError::new("SERVICE_HOST_THREAD_FAILED"))?;

        let deadline = Instant::now() + Duration::from_secs(60);
        let mut checkpoint = 1;
        loop {
            if let Ok(result) = result_rx.try_recv() {
                return stop_service(&status, result);
            }
            match ready_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(()) => break,
                Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => {
                    checkpoint += 1;
                    status
                        .set_service_status(ServiceStatus {
                            service_type: SERVICE_TYPE,
                            current_state: ServiceState::StartPending,
                            controls_accepted: ServiceControlAccept::empty(),
                            exit_code: ServiceExitCode::Win32(0),
                            checkpoint,
                            wait_hint: Duration::from_secs(20),
                            process_id: None,
                        })
                        .map_err(|_| PlatformError::unavailable())?;
                }
                Err(_) => {
                    let result = result_rx
                        .recv()
                        .unwrap_or_else(|_| Err(PlatformError::new("SERVICE_READINESS_FAILED")));
                    return stop_service(&status, result);
                }
            }
        }
        status
            .set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::ZERO,
                process_id: None,
            })
            .map_err(|_| PlatformError::unavailable())?;

        let result = result_rx
            .recv()
            .unwrap_or_else(|_| Err(PlatformError::new("SERVICE_HOST_STOPPED")));
        stop_service(&status, result)
    }

    fn stop_service(status: &ServiceStatusHandle, result: PlatformResult<()>) -> PlatformResult<()> {
        let exit_code = if result.is_ok() {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::ServiceSpecific(1)
        };
        let _ = status.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code,
            checkpoint: 0,
            wait_hint: Duration::ZERO,
            process_id: None,
        });
        result
    }
}

#[cfg(windows)]
pub use windows::run_dispatcher;

#[cfg(not(windows))]
pub fn run_dispatcher(_host: Arc<dyn ServiceHost>) -> PlatformResult<()> {
    Err(PlatformError::unavailable())
}
