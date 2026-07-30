use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, FromRef, Path as AxumPath, State, rejection::JsonRejection},
    http::{
        HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, Uri,
        header::{self, HOST, ORIGIN},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::{Deserialize, Serialize};
use time::Duration as CookieDuration;
use tokio::{net::TcpListener, task};
use tower_http::{catch_panic::CatchPanicLayer, limit::RequestBodyLimitLayer};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{
    application::{AuthenticationService, SetupService},
    domain::{
        AppError, AppResult, AuthSession, ConfirmSetupInput, InitialSetupInput, LoginInput,
        MasterUserInput, OrganizationInput, SetupProgress, StartupState, StorageInput, UnitInput,
    },
};

const SESSION_COOKIE: &str = "__Host-dental_session";
const CSRF_HEADER: &str = "x-csrf-token";
const CORRELATION_HEADER: &str = "x-correlation-id";
const MAX_JSON_BODY: usize = 64 * 1024;
const MAX_RATE_LIMIT_ADDRESSES: usize = 1024;

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

pub use platform_adapters::{PlatformPairingAdapter, PlatformStorageVolumeAdapter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageVolume {
    pub id: String,
    pub root_path: String,
    pub label: String,
    pub file_system: String,
    pub available_bytes: u64,
    pub kind: String,
    pub writable: bool,
    pub destination_path: String,
}

pub trait StorageVolumePort: Send + Sync {
    fn enumerate(&self) -> AppResult<Vec<StorageVolume>>;
    fn resolve_artifact_directories(
        &self,
        backup_volume_id: &str,
        recovery_volume_id: &str,
        active_directory: &Path,
    ) -> AppResult<StorageInput>;
}

pub trait PairingPort: Send + Sync {
    fn start(&self) -> AppResult<PairingStartResponse>;
    fn complete(&self, token: &str) -> AppResult<PairingCompleteResponse>;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingStartResponse {
    pub token: String,
    pub pairing_url: String,
    pub fingerprint_sha256: String,
    pub expires_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingCompleteResponse {
    pub fingerprint_sha256: String,
    pub ca_certificate_der_base64: String,
    pub ca_file_name: String,
    pub server_url: String,
}

#[derive(Clone)]
pub struct HttpState {
    setup: Arc<SetupService>,
    auth: Arc<AuthenticationService>,
    volumes: Arc<dyn StorageVolumePort>,
    pairing: Option<Arc<dyn PairingPort>>,
    login_limiter: Arc<LoginRateLimiter>,
    pairing_limiter: Arc<LoginRateLimiter>,
    login_hash_slots: Arc<tokio::sync::Semaphore>,
    ready: tokio::sync::watch::Sender<bool>,
}

impl HttpState {
    pub fn new(setup: SetupService, volumes: Arc<dyn StorageVolumePort>) -> AppResult<Self> {
        let setup = Arc::new(setup);
        let is_ready = lan_ready(&setup.get_startup_state()?);
        let auth = Arc::new(AuthenticationService::new(setup.database_worker()));
        let (ready, _) = tokio::sync::watch::channel(is_ready);
        Ok(Self {
            setup,
            auth,
            volumes,
            pairing: None,
            login_limiter: Arc::new(LoginRateLimiter::default()),
            pairing_limiter: Arc::new(LoginRateLimiter::default()),
            login_hash_slots: Arc::new(tokio::sync::Semaphore::new(3)),
            ready,
        })
    }

    pub fn with_pairing(mut self, pairing: Arc<dyn PairingPort>) -> Self {
        self.pairing = Some(pairing);
        self
    }

    pub fn ready_receiver(&self) -> tokio::sync::watch::Receiver<bool> {
        self.ready.subscribe()
    }
}

impl FromRef<HttpState> for Arc<SetupService> {
    fn from_ref(state: &HttpState) -> Self {
        state.setup.clone()
    }
}

#[derive(Clone)]
struct OriginPolicy {
    allowed_hosts: Arc<HashSet<String>>,
    scheme: &'static str,
}

impl OriginPolicy {
    fn new(hosts: impl IntoIterator<Item = String>, scheme: &'static str) -> Self {
        Self {
            allowed_hosts: Arc::new(
                hosts
                    .into_iter()
                    .map(|host| host.to_ascii_lowercase())
                    .collect(),
            ),
            scheme,
        }
    }
}

pub fn admin_router(state: HttpState, port: u16) -> Router {
    let policy = OriginPolicy::new(
        [format!("127.0.0.1:{port}"), format!("localhost:{port}")],
        "http",
    );
    Router::new()
        .route("/api/v1/setup/state", get(get_setup_state))
        .route("/api/v1/health", get(health))
        .route("/api/v1/setup/storage-volumes", get(get_storage_volumes))
        .route("/api/v1/setup/start", post(start_setup))
        .route("/api/v1/setup/{setup_id}/resume", post(resume_setup))
        .route("/api/v1/setup/{setup_id}/confirm", post(confirm_setup))
        .route("/api/v1/pairing/start", post(start_pairing))
        .fallback(spa)
        .with_state(state)
        .layer(middleware::from_fn_with_state(policy, enforce_origin))
        .layer(RequestBodyLimitLayer::new(MAX_JSON_BODY))
        .layer(CatchPanicLayer::new())
        .layer(middleware::from_fn(normalize_api_response))
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(correlation_id))
}

pub fn lan_router(state: HttpState, allowed_hosts: Vec<String>) -> Router {
    let policy = OriginPolicy::new(allowed_hosts, "https");
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/session", get(session))
        .route("/api/v1/auth/csrf/rotate", post(rotate_csrf))
        .route("/api/v1/pairing/complete", post(complete_pairing))
        .fallback(spa)
        .with_state(state)
        .layer(middleware::from_fn_with_state(policy, enforce_origin))
        .layer(RequestBodyLimitLayer::new(MAX_JSON_BODY))
        .layer(CatchPanicLayer::new())
        .layer(middleware::from_fn(normalize_api_response))
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(hsts))
        .layer(middleware::from_fn(correlation_id))
}

pub async fn serve_admin(
    listener: TcpListener,
    state: HttpState,
    port: u16,
) -> std::io::Result<()> {
    axum::serve(
        listener,
        admin_router(state, port).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

#[derive(Serialize)]
struct StorageVolumesResponse {
    volumes: Vec<StorageVolume>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StorageVolumeSelection {
    backup_volume_id: String,
    recovery_volume_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitialSetupHttpInput {
    organization: OrganizationInput,
    unit: UnitInput,
    master: MasterUserInput,
    storage: StorageVolumeSelection,
}

#[derive(Deserialize)]
struct ResumeSetupHttpInput {
    storage: StorageVolumeSelection,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct ConfirmSetupHttpInput {
    recovery_code: String,
    acknowledged_separate_storage: bool,
    acknowledged_loss_risk: bool,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct PairingCompleteInput {
    token: String,
}

async fn get_setup_state(State(state): State<HttpState>) -> ApiResult<Json<StartupState>> {
    let setup = state.setup.clone();
    blocking(move || setup.get_startup_state()).await.map(Json)
}

async fn get_storage_volumes(
    State(state): State<HttpState>,
) -> ApiResult<Json<StorageVolumesResponse>> {
    let provider = state.volumes.clone();
    blocking(move || provider.enumerate())
        .await
        .map(|volumes| Json(StorageVolumesResponse { volumes }))
}

async fn start_setup(
    State(state): State<HttpState>,
    payload: Result<Json<InitialSetupHttpInput>, JsonRejection>,
) -> ApiResult<Json<SetupProgress>> {
    let payload = json_payload(payload)?;
    let setup = state.setup.clone();
    let provider = state.volumes.clone();
    blocking(move || {
        let storage = resolve_storage(&*provider, &payload.storage, setup.active_directory())?;
        setup.start_initial_setup(InitialSetupInput {
            organization: payload.organization,
            unit: payload.unit,
            master: payload.master,
            storage,
        })
    })
    .await
    .map(Json)
}

async fn resume_setup(
    State(state): State<HttpState>,
    AxumPath(setup_id): AxumPath<String>,
    payload: Result<Json<ResumeSetupHttpInput>, JsonRejection>,
) -> ApiResult<Json<SetupProgress>> {
    let payload = json_payload(payload)?;
    let setup = state.setup.clone();
    let provider = state.volumes.clone();
    blocking(move || {
        let storage = resolve_storage(&*provider, &payload.storage, setup.active_directory())?;
        setup.resume_initial_setup(setup_id, Some(storage))
    })
    .await
    .map(Json)
}

async fn confirm_setup(
    State(state): State<HttpState>,
    AxumPath(setup_id): AxumPath<String>,
    payload: Result<Json<ConfirmSetupHttpInput>, JsonRejection>,
) -> ApiResult<Json<StartupState>> {
    let mut payload = json_payload(payload)?;
    let setup = state.setup.clone();
    let result = blocking(move || {
        setup.confirm_initial_setup(ConfirmSetupInput {
            setup_id,
            recovery_code: std::mem::take(&mut payload.recovery_code),
            acknowledged_separate_storage: payload.acknowledged_separate_storage,
            acknowledged_loss_risk: payload.acknowledged_loss_risk,
        })
    })
    .await?;
    if lan_ready(&result) {
        state.ready.send_replace(true);
    }
    Ok(Json(result))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn start_pairing(State(state): State<HttpState>) -> ApiResult<Json<PairingStartResponse>> {
    let pairing = state.pairing.clone().ok_or_else(|| {
        ApiError::from(AppError::new(
            "PAIRING_UNAVAILABLE",
            "O pareamento não está disponível neste momento.",
        ))
    })?;
    let setup = state.setup.clone();
    blocking(move || {
        ensure_lan_ready(&setup)?;
        pairing.start()
    })
    .await
    .map(Json)
}

async fn complete_pairing(
    State(state): State<HttpState>,
    peer: ConnectInfo<SocketAddr>,
    payload: Result<Json<PairingCompleteInput>, JsonRejection>,
) -> ApiResult<Json<PairingCompleteResponse>> {
    let mut payload = json_payload(payload)?;
    let pairing = state.pairing.clone().ok_or_else(|| {
        ApiError::from(AppError::new(
            "PAIRING_UNAVAILABLE",
            "O pareamento não está disponível neste momento.",
        ))
    })?;
    state
        .pairing_limiter
        .ensure_allowed(peer.0.ip())
        .map_err(|error| {
            if error.code == "TOO_MANY_LOGIN_ATTEMPTS" {
                ApiError::from(AppError::pairing_rate_limited())
            } else {
                ApiError::from(error)
            }
        })?;
    let token = zeroize::Zeroizing::new(std::mem::take(&mut payload.token));
    let result = blocking(move || pairing.complete(&token)).await;
    match result {
        Ok(response) => {
            state.pairing_limiter.clear(peer.0.ip());
            Ok(Json(response))
        }
        Err(error) => {
            if error.0.code == "PAIRING_TOKEN_INVALID" {
                state.pairing_limiter.record_failure(peer.0.ip());
            }
            Err(error)
        }
    }
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn login(
    State(state): State<HttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    payload: Result<Json<LoginInput>, JsonRejection>,
) -> ApiResult<Response> {
    let payload = json_payload(payload)?;
    let peer_ip = peer.ip();
    state.login_limiter.ensure_allowed(peer_ip)?;
    let hash_permit = state
        .login_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::from(AppError::rate_limited()))?;
    let setup = state.setup.clone();
    let auth = state.auth.clone();
    let result = blocking(move || {
        let _hash_permit = hash_permit;
        ensure_ready(&setup)?;
        auth.login(payload)
    })
    .await;
    let issued = match result {
        Ok(value) => {
            state.login_limiter.clear(peer_ip);
            value
        }
        Err(error) if error.0.code == "INVALID_CREDENTIALS" => {
            state.login_limiter.record_failure(peer_ip);
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let response = (
        jar.add(session_cookie(&issued.session_token)),
        Json(issued.public),
    )
        .into_response();
    with_csrf_header(response, &issued.csrf_token)
}

async fn session(State(state): State<HttpState>, jar: CookieJar) -> ApiResult<Response> {
    let Some(session_token) = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value().to_owned())
    else {
        return Ok((
            clear_auth_cookies(jar),
            Json(AuthSession::unauthenticated()),
        )
            .into_response());
    };
    let setup = state.setup.clone();
    let auth = state.auth.clone();
    let result = blocking(move || {
        ensure_ready(&setup)?;
        auth.session(&session_token)
    })
    .await;
    match result {
        Ok(view) => with_csrf_header(Json(view.public).into_response(), &view.csrf_token),
        Err(error) if error.0.code == "AUTHENTICATION_REQUIRED" => Ok((
            clear_auth_cookies(jar),
            Json(AuthSession::unauthenticated()),
        )
            .into_response()),
        Err(error) => Err(error),
    }
}

async fn logout(
    State(state): State<HttpState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> ApiResult<(CookieJar, StatusCode)> {
    let Some(session_token) = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value().to_owned())
    else {
        return Ok((clear_auth_cookies(jar), StatusCode::NO_CONTENT));
    };
    let csrf = csrf_header(&headers)?;
    let setup = state.setup.clone();
    let auth = state.auth.clone();
    blocking(move || {
        ensure_ready(&setup)?;
        auth.logout(&session_token, &csrf)
    })
    .await?;
    Ok((clear_auth_cookies(jar), StatusCode::NO_CONTENT))
}

async fn rotate_csrf(
    State(state): State<HttpState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let session_token = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(|| ApiError::from(AppError::unauthenticated()))?;
    let csrf = csrf_header(&headers)?;
    let setup = state.setup.clone();
    let auth = state.auth.clone();
    let issued = blocking(move || {
        ensure_ready(&setup)?;
        auth.rotate(&session_token, &csrf)
    })
    .await?;
    let response = (
        jar.add(session_cookie(&issued.session_token)),
        StatusCode::NO_CONTENT,
    )
        .into_response();
    with_csrf_header(response, &issued.csrf_token)
}

fn resolve_storage(
    provider: &dyn StorageVolumePort,
    selection: &StorageVolumeSelection,
    active_directory: &Path,
) -> AppResult<StorageInput> {
    if selection.backup_volume_id == selection.recovery_volume_id {
        return Err(AppError::validation(vec![crate::domain::FieldError {
            field: "storage".to_owned(),
            message: "Escolha volumes diferentes para backup e recuperação.".to_owned(),
        }]));
    }
    provider.resolve_artifact_directories(
        &selection.backup_volume_id,
        &selection.recovery_volume_id,
        active_directory,
    )
}

fn ensure_ready(setup: &SetupService) -> AppResult<()> {
    match setup.get_startup_state()? {
        StartupState::Ready { .. } => Ok(()),
        _ => Err(AppError::new(
            "SERVICE_NOT_READY",
            "O sistema ainda não está disponível para autenticação.",
        )),
    }
}

fn ensure_lan_ready(setup: &SetupService) -> AppResult<()> {
    if lan_ready(&setup.get_startup_state()?) {
        Ok(())
    } else {
        Err(AppError::new(
            "SERVICE_NOT_READY",
            "O sistema ainda não está disponível na rede local.",
        ))
    }
}

fn lan_ready(state: &StartupState) -> bool {
    matches!(
        state,
        StartupState::Ready { diagnostics } if diagnostics.distribution_ready
    )
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> ApiResult<T> {
    task::spawn_blocking(operation)
        .await
        .map_err(|_| ApiError::from(AppError::worker()))?
        .map_err(ApiError::from)
}

fn json_payload<T>(payload: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    match payload {
        Ok(Json(value)) => Ok(value),
        Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            Err(payload_too_large_error())
        }
        Err(_) => Err(ApiError::from(AppError::new(
            "INVALID_JSON",
            "O corpo da solicitação não contém um JSON válido.",
        ))),
    }
}

fn payload_too_large_error() -> ApiError {
    ApiError::from(AppError::new(
        "PAYLOAD_TOO_LARGE",
        "O corpo da solicitação excede o limite permitido.",
    ))
    .with_status(StatusCode::PAYLOAD_TOO_LARGE)
}

fn csrf_header(headers: &HeaderMap) -> ApiResult<String> {
    let value = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value.len() == 43
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
        .ok_or_else(|| {
            ApiError::from(AppError::forbidden(
                "CSRF_VALIDATION_FAILED",
                "A validação de segurança da solicitação falhou.",
            ))
        })?;
    Ok(value.to_owned())
}

fn clear_auth_cookies(jar: CookieJar) -> CookieJar {
    jar.remove(expired_cookie(SESSION_COOKIE))
}

fn session_cookie(value: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, value.to_owned()))
        .path("/")
        .secure(true)
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(CookieDuration::hours(12))
        .build()
}

fn expired_cookie(name: &'static str) -> Cookie<'static> {
    Cookie::build((name, ""))
        .path("/")
        .secure(true)
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(CookieDuration::ZERO)
        .build()
}

fn with_csrf_header(mut response: Response, csrf_token: &str) -> ApiResult<Response> {
    let value =
        HeaderValue::from_str(csrf_token).map_err(|_| ApiError::from(AppError::security()))?;
    response
        .headers_mut()
        .insert(HeaderName::from_static(CSRF_HEADER), value);
    Ok(response)
}

async fn enforce_origin(
    State(policy): State<OriginPolicy>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase);
    let host_allowed = host
        .as_ref()
        .is_some_and(|value| policy.allowed_hosts.contains(value));
    if !host_allowed {
        return ApiError::from(AppError::forbidden(
            "HOST_VALIDATION_FAILED",
            "O destino da solicitação não é permitido.",
        ))
        .into_response();
    }

    if is_mutating(request.method()) {
        let expected = format!(
            "{}://{}",
            policy.scheme,
            host.as_deref().unwrap_or_default()
        );
        let origin_allowed = request
            .headers()
            .get(ORIGIN)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case(&expected));
        if !origin_allowed {
            return ApiError::from(AppError::forbidden(
                "ORIGIN_VALIDATION_FAILED",
                "A origem da solicitação não é permitida.",
            ))
            .into_response();
        }
    }
    next.run(request).await
}

fn is_mutating(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

async fn correlation_id(mut request: Request<Body>, next: Next) -> Response {
    let correlation_id = Uuid::now_v7().to_string();
    request.extensions_mut().insert(correlation_id.clone());
    let mut response = next.run(request).await;
    if !response.headers().contains_key(CORRELATION_HEADER)
        && let Ok(value) = HeaderValue::from_str(&correlation_id)
    {
        response
            .headers_mut()
            .insert(HeaderName::from_static(CORRELATION_HEADER), value);
    }
    response
}

async fn normalize_api_response(request: Request<Body>, next: Next) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    let response = next.run(request).await;
    if !is_api
        || response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|value| {
                value
                    .to_str()
                    .ok()
                    .is_some_and(|content_type| content_type.starts_with("application/json"))
            })
    {
        return response;
    }

    let normalized = match response.status() {
        StatusCode::PAYLOAD_TOO_LARGE => Some(payload_too_large_error()),
        StatusCode::METHOD_NOT_ALLOWED => Some(
            ApiError::from(AppError::new(
                "METHOD_NOT_ALLOWED",
                "O método solicitado não é permitido.",
            ))
            .with_status(StatusCode::METHOD_NOT_ALLOWED),
        ),
        StatusCode::INTERNAL_SERVER_ERROR => Some(
            ApiError::from(AppError::new(
                "INTERNAL_SERVER_ERROR",
                "Não foi possível concluir a solicitação.",
            ))
            .with_status(StatusCode::INTERNAL_SERVER_ERROR),
        ),
        _ => None,
    };
    normalized.map_or(response, IntoResponse::into_response)
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; base-uri 'self'; connect-src 'self'; font-src 'self'; form-action 'self'; frame-ancestors 'none'; img-src 'self' data:; manifest-src 'self'; object-src 'none'; script-src 'self'; style-src 'self'; worker-src 'self'",
        ),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static(
            "camera=(), display-capture=(), geolocation=(), microphone=(), payment=(), usb=()",
        ),
    );
    if is_api {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    }
    response
}

async fn hsts(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000"),
    );
    response
}

async fn spa(method: Method, uri: Uri) -> Response {
    if !matches!(method, Method::GET | Method::HEAD) {
        return ApiError::from(AppError::new(
            "METHOD_NOT_ALLOWED",
            "O método solicitado não é permitido.",
        ))
        .with_status(StatusCode::METHOD_NOT_ALLOWED)
        .into_response();
    }
    let is_head = method == Method::HEAD;
    let path = uri.path().trim_start_matches('/');
    let requested = if path.is_empty() { "index.html" } else { path };
    let asset = embedded_asset(requested).or_else(|| {
        (!requested.starts_with("api/") && Path::new(requested).extension().is_none())
            .then(|| embedded_asset("index.html"))
            .flatten()
    });
    match asset {
        Some((bytes, content_type)) => {
            let cache = if requested.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
                    (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
                ],
                if is_head {
                    Body::empty()
                } else {
                    Body::from(bytes)
                },
            )
                .into_response()
        }
        None => ApiError::from(AppError::new(
            "ROUTE_NOT_FOUND",
            "O recurso solicitado não foi encontrado.",
        ))
        .with_status(StatusCode::NOT_FOUND)
        .into_response(),
    }
}

type ApiResult<T> = Result<T, ApiError>;

pub struct ApiError(AppError, Option<StatusCode>);

impl ApiError {
    fn with_status(self, status: StatusCode) -> Self {
        Self(self.0, Some(status))
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        Self(error, None)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.1.unwrap_or_else(|| status_for_error(&self.0.code));
        let correlation_id = self.0.correlation_id.clone();
        let mut response = (status, Json(self.0)).into_response();
        if let Ok(value) = HeaderValue::from_str(&correlation_id) {
            response
                .headers_mut()
                .insert(HeaderName::from_static(CORRELATION_HEADER), value);
        }
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        }
        response
    }
}

fn status_for_error(code: &str) -> StatusCode {
    match code {
        "INVALID_JSON" | "VALIDATION_FAILED" => StatusCode::UNPROCESSABLE_ENTITY,
        "INVALID_CREDENTIALS" | "AUTHENTICATION_REQUIRED" | "PAIRING_TOKEN_INVALID" => {
            StatusCode::UNAUTHORIZED
        }
        "CSRF_VALIDATION_FAILED" | "HOST_VALIDATION_FAILED" | "ORIGIN_VALIDATION_FAILED" => {
            StatusCode::FORBIDDEN
        }
        "INVALID_STATE" => StatusCode::CONFLICT,
        "TOO_MANY_LOGIN_ATTEMPTS" | "TOO_MANY_PAIRING_ATTEMPTS" => StatusCode::TOO_MANY_REQUESTS,
        "SERVICE_NOT_READY" | "PLATFORM_STORAGE_UNAVAILABLE" | "PAIRING_UNAVAILABLE" => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Default)]
struct LoginRateLimiter {
    failures: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl LoginRateLimiter {
    fn ensure_allowed(&self, address: IpAddr) -> AppResult<()> {
        let mut failures = self.failures.lock().map_err(|_| AppError::worker())?;
        prune_failure_map(&mut failures);
        if failures
            .get(&address)
            .is_some_and(|entries| entries.len() >= 10)
            || (!failures.contains_key(&address) && failures.len() >= MAX_RATE_LIMIT_ADDRESSES)
        {
            return Err(AppError::rate_limited());
        }
        Ok(())
    }

    fn record_failure(&self, address: IpAddr) {
        if let Ok(mut failures) = self.failures.lock() {
            prune_failure_map(&mut failures);
            if !failures.contains_key(&address) && failures.len() >= MAX_RATE_LIMIT_ADDRESSES {
                return;
            }
            let entries = failures.entry(address).or_default();
            entries.push_back(Instant::now());
        }
    }

    fn clear(&self, address: IpAddr) {
        if let Ok(mut failures) = self.failures.lock() {
            failures.remove(&address);
        }
    }
}

fn prune_failure_map(failures: &mut HashMap<IpAddr, VecDeque<Instant>>) {
    failures.retain(|_, entries| {
        prune_failures(entries);
        !entries.is_empty()
    });
}

fn prune_failures(entries: &mut VecDeque<Instant>) {
    let cutoff = Instant::now() - Duration::from_secs(5 * 60);
    while entries.front().is_some_and(|instant| *instant < cutoff) {
        entries.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::Request};
    use std::net::Ipv4Addr;
    use tower::ServiceExt;

    struct NoVolumes;

    impl StorageVolumePort for NoVolumes {
        fn enumerate(&self) -> AppResult<Vec<StorageVolume>> {
            Ok(Vec::new())
        }

        fn resolve_artifact_directories(
            &self,
            _backup_volume_id: &str,
            _recovery_volume_id: &str,
            _active_directory: &Path,
        ) -> AppResult<StorageInput> {
            Err(AppError::new(
                "PLATFORM_STORAGE_UNAVAILABLE",
                "O armazenamento local não está disponível.",
            ))
        }
    }

    fn state() -> (tempfile::TempDir, HttpState) {
        let directory = tempfile::tempdir().expect("tempdir");
        let setup = SetupService::new(directory.path().to_path_buf());
        (
            directory,
            HttpState::new(setup, Arc::new(NoVolumes)).expect("http state"),
        )
    }

    #[tokio::test]
    async fn lan_router_does_not_expose_setup_routes() {
        let (_directory, state) = state();
        let response = lan_router(state, vec!["dental-test.local:8743".to_owned()])
            .oneshot(
                Request::builder()
                    .uri("/api/v1/setup/state")
                    .header(HOST, "dental-test.local:8743")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn admin_mutation_rejects_cross_origin_requests() {
        let (_directory, state) = state();
        let response = admin_router(state, 8742)
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/setup/start")
                    .header(HOST, "127.0.0.1:8742")
                    .header(ORIGIN, "https://attacker.example")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn admin_rejects_dns_rebinding_host() {
        let (_directory, state) = state();
        let response = admin_router(state, 8742)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/setup/state")
                    .header(HOST, "attacker.example")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn infrastructure_rejections_keep_the_sanitized_error_contract() {
        let (_directory, state) = state();
        let oversized = admin_router(state.clone(), 8742)
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/setup/start")
                    .header(HOST, "127.0.0.1:8742")
                    .header(ORIGIN, "http://127.0.0.1:8742")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(vec![b' '; MAX_JSON_BODY + 1]))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error_code(oversized).await, "PAYLOAD_TOO_LARGE");

        let wrong_method = admin_router(state, 8742)
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/setup/start")
                    .header(HOST, "127.0.0.1:8742")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(error_code(wrong_method).await, "METHOD_NOT_ALLOWED");
    }

    #[tokio::test]
    async fn caught_panics_are_returned_as_sanitized_json() {
        async fn panic_handler() -> StatusCode {
            panic!("internal details must not cross the HTTP boundary")
        }

        let response = Router::new()
            .route("/api/v1/panic", get(panic_handler))
            .layer(CatchPanicLayer::new())
            .layer(middleware::from_fn(normalize_api_response))
            .oneshot(
                Request::builder()
                    .uri("/api/v1/panic")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error_code(response).await, "INTERNAL_SERVER_ERROR");
    }

    #[test]
    fn login_rate_limit_is_bounded_and_resettable() {
        let limiter = LoginRateLimiter::default();
        let address = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        for _ in 0..10 {
            limiter.ensure_allowed(address).expect("still allowed");
            limiter.record_failure(address);
        }
        assert_eq!(
            limiter
                .ensure_allowed(address)
                .expect_err("rate limited")
                .code,
            "TOO_MANY_LOGIN_ATTEMPTS"
        );
        limiter.clear(address);
        limiter.ensure_allowed(address).expect("reset");
    }

    #[test]
    fn rate_limit_memory_is_bounded_by_address_count() {
        let limiter = LoginRateLimiter::default();
        for suffix in 0..MAX_RATE_LIMIT_ADDRESSES {
            let address = IpAddr::V6(std::net::Ipv6Addr::new(
                0x2001,
                0xdb8,
                0,
                0,
                0,
                0,
                (suffix >> 16) as u16,
                suffix as u16,
            ));
            limiter.ensure_allowed(address).expect("address allowed");
            limiter.record_failure(address);
        }
        assert_eq!(
            limiter.failures.lock().expect("rate limiter").len(),
            MAX_RATE_LIMIT_ADDRESSES
        );
        assert_eq!(
            limiter
                .ensure_allowed(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)))
                .expect_err("new address blocked at capacity")
                .code,
            "TOO_MANY_LOGIN_ATTEMPTS"
        );
    }

    #[test]
    fn argon2_concurrency_is_bounded_before_hashing() {
        let (_directory, state) = state();
        let first = state
            .login_hash_slots
            .clone()
            .try_acquire_owned()
            .expect("slot 1");
        let second = state
            .login_hash_slots
            .clone()
            .try_acquire_owned()
            .expect("slot 2");
        let third = state
            .login_hash_slots
            .clone()
            .try_acquire_owned()
            .expect("slot 3");
        assert!(state.login_hash_slots.clone().try_acquire_owned().is_err());
        drop((first, second, third));
        assert!(state.login_hash_slots.clone().try_acquire_owned().is_ok());
    }

    async fn error_code(response: Response) -> String {
        let body = to_bytes(response.into_body(), MAX_JSON_BODY)
            .await
            .expect("error body");
        serde_json::from_slice::<serde_json::Value>(&body)
            .expect("JSON error")
            .get("code")
            .and_then(serde_json::Value::as_str)
            .expect("error code")
            .to_owned()
    }
}
mod platform_adapters;
