pub mod application;
pub mod domain;
pub mod http;
pub mod infrastructure;
pub mod platform;

use std::{
    env,
    ffi::OsStr,
    future::Future,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Once,
        mpsc::{Receiver, SyncSender},
    },
    time::Duration,
};

use axum_server::tls_rustls::RustlsConfig;
use chrono::Utc;
use http::{HttpState, PlatformPairingAdapter, PlatformStorageVolumeAdapter};
use infrastructure::PlatformKeyProtector;
use platform::{
    PlatformError, PlatformResult,
    discovery::MdnsRegistration,
    host_identity::HostIdentityManager,
    instance_lock::InstanceGuard,
    pairing::PairingManager,
    tls::{TlsIdentityAction, TlsIdentityManager},
};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{net::TcpListener, sync::watch, task::JoinHandle};
use uuid::Uuid;

pub const DEFAULT_ADMIN_PORT: u16 = 8742;
pub const DEFAULT_LAN_PORT: u16 = 8743;

const STARTUP_ERROR_CODES: &[&str] = &[
    "CONSOLE_DATA_DIRECTORY_INVALID",
    "CONSOLE_DATA_DIRECTORY_RESERVED",
    "HOST_TASK_STOPPED",
    "INSTANCE_ALREADY_RUNNING",
    "SERVICE_HOST_STOP_TIMEOUT",
    "SERVICE_READINESS_FAILED",
    "SERVICE_READINESS_TIMEOUT",
    "STARTUP_ADMIN_BIND_FAILED",
    "STARTUP_DATA_DIRECTORY_FAILED",
    "STARTUP_DATABASE_FAILED",
    "STARTUP_HOST_IDENTITY_FAILED",
    "STARTUP_INSTANCE_LOCK_FAILED",
    "STARTUP_RUNTIME_FAILED",
    "STARTUP_SERVICE_READY_CHANNEL_FAILED",
    "STARTUP_TLS_FAILED",
];

pub fn startup_error_code(error: &(dyn std::error::Error + Send + Sync + 'static)) -> &'static str {
    let displayed = error.to_string();
    STARTUP_ERROR_CODES
        .iter()
        .copied()
        .find(|code| *code == displayed)
        .unwrap_or("STARTUP_RUNTIME_FAILED")
}

pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let product_root = default_product_root()?;
    let data_directory = product_root.join("Data");
    run_with_paths(product_root, data_directory, "service", None, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

pub async fn run_console(
    data_directory: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (product_root, data_directory) = validate_console_data_directory(&data_directory)?;
    run_with_paths(product_root, data_directory, "console", None, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

pub fn security_diagnostics_json() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let diagnostics = infrastructure::runtime_security_diagnostics()?;
    Ok(serde_json::to_string(&diagnostics)?)
}

pub fn startup_diagnostics_json() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(windows)]
    let product_root = env::var_os("ProgramData")
        .map(PathBuf::from)
        .map(|path| path.join("OfflineDentalSystem"));
    #[cfg(not(windows))]
    let product_root = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .map(|path| path.join("offline-dental-system"));

    let diagnostics =
        platform::startup_diagnostics::collect(product_root.as_deref(), product_root.is_some());
    Ok(serde_json::to_string(&diagnostics)?)
}

pub fn run_as_windows_service(
    readiness: SyncSender<()>,
    shutdown: Receiver<()>,
) -> PlatformResult<()> {
    let product_root =
        default_product_root().map_err(|_| PlatformError::new("SERVICE_DATA_ROOT_UNAVAILABLE"))?;
    let data_directory = product_root.join("Data");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("ods-service-stop".to_owned())
        .spawn(move || {
            let _ = shutdown.recv();
            let _ = shutdown_tx.send(());
        })
        .map_err(|_| PlatformError::new("SERVICE_STOP_BRIDGE_FAILED"))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| PlatformError::new("SERVICE_RUNTIME_FAILED"))?;
    runtime
        .block_on(run_with_paths(
            product_root,
            data_directory,
            "service",
            Some(readiness),
            async move {
                let _ = shutdown_rx.await;
            },
        ))
        .map_err(|error| PlatformError::new(startup_error_code(error.as_ref())))
}

async fn run_with_paths(
    product_root: PathBuf,
    data_directory: PathBuf,
    mode: &'static str,
    readiness: Option<SyncSender<()>>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let run_id = Uuid::now_v7();
    init_tracing(&product_root, run_id, mode)?;
    tracing::info!(event = "PRODUCT_ROOT_RESOLVED", run_id = %run_id, mode);
    std::fs::create_dir_all(&data_directory)
        .map_err(|_| startup_failed("STARTUP_DATA_DIRECTORY_FAILED"))?;
    tracing::info!(event = "DATA_DIRECTORY_READY", run_id = %run_id);
    let _ = rustls::crypto::ring::default_provider().install_default();

    let _instance = InstanceGuard::acquire(&product_root)
        .map_err(|_| startup_failed("STARTUP_INSTANCE_LOCK_FAILED"))?;
    tracing::info!(event = "INSTANCE_LOCK_ACQUIRED", run_id = %run_id);
    let host = HostIdentityManager::load_or_create(&product_root)
        .map_err(|_| startup_failed("STARTUP_HOST_IDENTITY_FAILED"))?;
    tracing::info!(event = "HOST_IDENTITY_READY", run_id = %run_id);
    let installation_id = Uuid::parse_str(&host.installation_id)
        .map_err(|_| startup_failed("STARTUP_HOST_IDENTITY_FAILED"))?;

    let tls_manager = Arc::new(
        TlsIdentityManager::new(&product_root, PlatformKeyProtector::new())
            .map_err(|_| startup_failed("STARTUP_TLS_FAILED"))?,
    );
    let pairing_manager = Arc::new(PairingManager::new());
    let pairing = Arc::new(PlatformPairingAdapter::new(pairing_manager));
    match tls_manager.ensure_identity(&host.installation_id, &host.hostname, Utc::now()) {
        Ok(identity) => {
            pairing
                .update_identity(
                    host.hostname.clone(),
                    identity.ca_fingerprint_sha256,
                    identity.ca_certificate_der,
                )
                .map_err(|_| startup_failed("STARTUP_TLS_FAILED"))?;
            tracing::info!(event = "TLS_IDENTITY_READY", run_id = %run_id);
        }
        Err(_) => {
            tracing::error!(event = "TLS_IDENTITY_UNAVAILABLE", code = "STARTUP_TLS_FAILED", run_id = %run_id);
        }
    }

    let setup =
        application::SetupService::for_installation(data_directory.clone(), installation_id);
    let volumes = Arc::new(PlatformStorageVolumeAdapter::new(data_directory));
    let state = HttpState::new(setup, volumes)
        .map_err(|_| startup_failed("STARTUP_DATABASE_FAILED"))?
        .with_pairing(pairing.clone());
    tracing::info!(event = "DATABASE_STATE_READ", run_id = %run_id);
    let ready = state.ready_receiver();

    let admin_address = SocketAddr::from(([127, 0, 0, 1], DEFAULT_ADMIN_PORT));
    let admin_listener = TcpListener::bind(admin_address)
        .await
        .map_err(|_| startup_failed("STARTUP_ADMIN_BIND_FAILED"))?;
    tracing::info!(event = "ADMIN_LISTENER_BOUND", address = %admin_address, run_id = %run_id);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (failure_tx, mut failure_rx) = tokio::sync::mpsc::channel::<&'static str>(2);
    let admin_task = spawn_admin(
        admin_listener,
        state.clone(),
        shutdown_rx.clone(),
        failure_tx.clone(),
    );
    let lan_task = spawn_lan_supervisor(
        state,
        ready,
        shutdown_rx,
        tls_manager,
        pairing,
        host.installation_id,
        host.hostname,
    );

    if let Some(readiness) = readiness {
        readiness
            .send(())
            .map_err(|_| startup_failed("STARTUP_SERVICE_READY_CHANNEL_FAILED"))?;
    }
    tracing::info!(event = "SERVICE_READY", run_id = %run_id);

    tokio::select! {
        _ = shutdown => {
            tracing::info!(event = "SHUTDOWN_REQUESTED", run_id = %run_id);
        }
        failure = failure_rx.recv() => {
            let code = failure.unwrap_or("HOST_TASK_STOPPED");
            tracing::error!(code, "server task stopped unexpectedly");
            shutdown_tx.send_replace(true);
            wait_for_tasks(admin_task, lan_task).await;
            return Err(code.into());
        }
    }

    shutdown_tx.send_replace(true);
    wait_for_tasks(admin_task, lan_task).await;
    tracing::info!(event = "SHUTDOWN_COMPLETED", run_id = %run_id);
    Ok(())
}

fn startup_failed(code: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    tracing::error!(event = "STARTUP_FAILED", code);
    code.into()
}

fn spawn_admin(
    listener: TcpListener,
    state: HttpState,
    mut shutdown: watch::Receiver<bool>,
    failures: tokio::sync::mpsc::Sender<&'static str>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let stopping = shutdown.clone();
        let server = axum::serve(
            listener,
            http::admin_router(state, DEFAULT_ADMIN_PORT)
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            wait_for_shutdown(&mut shutdown).await;
        });
        let result = server.await;
        if result.is_err() || !*stopping.borrow() {
            let _ = failures.send("ADMIN_LISTENER_FAILED").await;
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn spawn_lan_supervisor(
    state: HttpState,
    mut ready: watch::Receiver<bool>,
    mut shutdown: watch::Receiver<bool>,
    tls_manager: Arc<TlsIdentityManager<PlatformKeyProtector>>,
    pairing: Arc<PlatformPairingAdapter>,
    installation_id: String,
    hostname: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let result = run_lan_supervisor(
                state.clone(),
                &mut ready,
                &mut shutdown,
                tls_manager.clone(),
                pairing.clone(),
                &installation_id,
                &hostname,
            )
            .await;
            if *shutdown.borrow() {
                return;
            }
            if result.is_err() {
                tracing::error!(
                    code = "LAN_LISTENER_FAILED",
                    "LAN remains closed; retry scheduled"
                );
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                _ = wait_for_shutdown(&mut shutdown) => return,
            }
        }
    })
}

async fn run_lan_supervisor(
    state: HttpState,
    ready: &mut watch::Receiver<bool>,
    shutdown: &mut watch::Receiver<bool>,
    tls_manager: Arc<TlsIdentityManager<PlatformKeyProtector>>,
    pairing: Arc<PlatformPairingAdapter>,
    installation_id: &str,
    hostname: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pairing.mark_unavailable();
    while !*ready.borrow() {
        tokio::select! {
            changed = ready.changed() => changed?,
            _ = wait_for_shutdown(shutdown) => return Ok(()),
        }
    }

    let manager = tls_manager.clone();
    let installation = installation_id.to_owned();
    let host = hostname.to_owned();
    let current_tls = tokio::task::spawn_blocking(move || {
        manager.ensure_identity(&installation, &host, Utc::now())
    })
    .await
    .map_err(|_| std::io::Error::other("TLS identity worker failed"))??;
    pairing.update_identity(
        hostname.to_owned(),
        current_tls.ca_fingerprint_sha256.clone(),
        current_tls.ca_certificate_der.clone(),
    )?;
    let tls_config = RustlsConfig::from_der(
        current_tls.certificate_chain_der,
        current_tls.private_key_der.to_vec(),
    )
    .await?;
    let listener = bind_dual_stack(DEFAULT_LAN_PORT)?;
    let lan_address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let server = axum_server::from_tcp_rustls(listener, tls_config.clone())?;

    let allowed_hosts = vec![
        format!("{hostname}:{DEFAULT_LAN_PORT}"),
        format!("localhost:{DEFAULT_LAN_PORT}"),
    ];
    let router =
        http::lan_router(state, allowed_hosts).into_make_service_with_connect_info::<SocketAddr>();
    let discovery = MdnsRegistration::register_if_ready(
        true,
        true,
        hostname,
        DEFAULT_LAN_PORT,
        installation_id,
    )?;
    let availability = PairingAvailabilityGuard::activate(pairing.clone());
    tracing::info!(event = "LAN_LISTENER_BOUND", address = %lan_address, service = discovery.fullname());

    let handle = axum_server::Handle::new();
    let serve = server.handle(handle.clone()).serve(router);
    tokio::pin!(serve);
    let mut renewal = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(24 * 60 * 60),
        Duration::from_secs(24 * 60 * 60),
    );
    renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            result = &mut serve => return result.map_err(Into::into),
            _ = wait_for_shutdown(shutdown) => {
                handle.graceful_shutdown(Some(Duration::from_secs(10)));
                return serve.await.map_err(Into::into);
            }
            _ = renewal.tick() => {
                let manager = tls_manager.clone();
                let installation_id = installation_id.to_owned();
                let renewed_hostname = hostname.to_owned();
                let worker_hostname = renewed_hostname.clone();
                let refreshed = tokio::task::spawn_blocking(move || {
                    manager.ensure_identity(&installation_id, &worker_hostname, Utc::now())
                }).await.map_err(|_| std::io::Error::other("TLS renewal worker failed"))??;
                if refreshed.action == TlsIdentityAction::Renewed {
                    availability.deactivate();
                    tls_config.reload_from_der(
                        refreshed.certificate_chain_der,
                        refreshed.private_key_der.to_vec(),
                    ).await?;
                    pairing.update_identity(
                        renewed_hostname,
                        refreshed.ca_fingerprint_sha256,
                        refreshed.ca_certificate_der,
                    )?;
                    availability.reactivate();
                    tracing::info!("TLS server certificate renewed");
                }
            }
        }
    }
}

fn bind_dual_stack(port: u16) -> std::io::Result<std::net::TcpListener> {
    let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_only_v6(false)?;
    if socket.only_v6()? {
        return Err(std::io::Error::other(
            "dual-stack listener could not be enabled",
        ));
    }
    socket.bind(&SocketAddr::from(([0_u16; 8], port)).into())?;
    socket.listen(128)?;
    Ok(socket.into())
}

struct PairingAvailabilityGuard {
    pairing: Arc<PlatformPairingAdapter>,
}

impl PairingAvailabilityGuard {
    fn activate(pairing: Arc<PlatformPairingAdapter>) -> Self {
        pairing.mark_operational();
        Self { pairing }
    }

    fn deactivate(&self) {
        self.pairing.mark_unavailable();
    }

    fn reactivate(&self) {
        self.pairing.mark_operational();
    }
}

impl Drop for PairingAvailabilityGuard {
    fn drop(&mut self) {
        self.pairing.mark_unavailable();
    }
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    while !*shutdown.borrow() {
        if shutdown.changed().await.is_err() {
            break;
        }
    }
}

async fn wait_for_tasks(admin: JoinHandle<()>, lan: JoinHandle<()>) {
    let _ = tokio::time::timeout(Duration::from_secs(15), async {
        let _ = tokio::join!(admin, lan);
    })
    .await;
}

fn default_product_root() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(windows)]
    {
        let program_data = env::var_os("ProgramData").ok_or("ProgramData is unavailable")?;
        Ok(PathBuf::from(program_data).join("OfflineDentalSystem"))
    }
    #[cfg(not(windows))]
    {
        let base = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .ok_or("local data directory is unavailable")?;
        Ok(base.join("offline-dental-system"))
    }
}

fn validate_console_data_directory(
    requested: &Path,
) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error + Send + Sync>> {
    if !requested.is_absolute()
        || requested
            .file_name()
            .and_then(OsStr::to_str)
            .is_none_or(|name| !name.eq_ignore_ascii_case("Data"))
        || !requested.is_dir()
    {
        return Err("CONSOLE_DATA_DIRECTORY_INVALID".into());
    }
    let data_directory = requested
        .canonicalize()
        .map_err(|_| "CONSOLE_DATA_DIRECTORY_INVALID")?;
    let product_root = data_directory
        .parent()
        .ok_or("CONSOLE_DATA_DIRECTORY_INVALID")?
        .to_path_buf();

    #[cfg(windows)]
    {
        let program_data = env::var_os("ProgramData")
            .map(PathBuf::from)
            .ok_or("PROGRAM_DATA_UNAVAILABLE")?
            .canonicalize()
            .map_err(|_| "PROGRAM_DATA_UNAVAILABLE")?;
        if data_directory.starts_with(program_data) {
            return Err("CONSOLE_DATA_DIRECTORY_RESERVED".into());
        }
    }

    Ok((product_root, data_directory))
}

fn init_tracing(
    product_root: &Path,
    run_id: Uuid,
    mode: &'static str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tracing_subscriber::fmt::writer::MakeWriterExt as _;

    static PANIC_HOOK: Once = Once::new();
    let log_directory = product_root.join("logs").join("runtime");
    std::fs::create_dir_all(&log_directory)?;
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("server")
        .filename_suffix("jsonl")
        .max_log_files(30)
        .build(log_directory)?;
    let filter = tracing_subscriber::EnvFilter::new("info,tower_http=warn");
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .flatten_event(true)
        .with_current_span(true)
        .with_ansi(false)
        .with_target(false)
        .with_writer(std::io::stderr.and(file_appender))
        .try_init();
    PANIC_HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|_| {
            tracing::error!(code = "UNHANDLED_PANIC", "an internal task panicked");
        }));
    });
    tracing::info!(event = "PROCESS_STARTED", run_id = %run_id, mode, version = env!("CARGO_PKG_VERSION"), retention_files = 30);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::{SocketAddr, TcpStream};

    use super::{bind_dual_stack, startup_error_code};

    #[test]
    fn startup_error_code_preserves_known_sanitized_codes() {
        let error: Box<dyn std::error::Error + Send + Sync> = "STARTUP_DATABASE_FAILED".into();

        assert_eq!(
            startup_error_code(error.as_ref()),
            "STARTUP_DATABASE_FAILED"
        );
    }

    #[test]
    fn startup_error_code_hides_unexpected_error_details() {
        let error = std::io::Error::other("C:\\sensitive\\database.sqlcipher");

        assert_eq!(startup_error_code(&error), "STARTUP_RUNTIME_FAILED");
    }

    #[test]
    fn lan_listener_accepts_ipv4_and_ipv6_before_mdns_can_be_enabled() {
        let listener = bind_dual_stack(0).expect("dual-stack listener");
        let port = listener.local_addr().expect("listener address").port();
        let acceptor = std::thread::spawn(move || {
            for _ in 0..2 {
                listener.accept().expect("accept client");
            }
        });

        let ipv4 =
            TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port))).expect("IPv4 connection");
        let ipv6 = TcpStream::connect(SocketAddr::from(([0_u16, 0, 0, 0, 0, 0, 0, 1], port)))
            .expect("IPv6 connection");
        drop((ipv4, ipv6));
        acceptor.join().expect("acceptor");
    }
}
