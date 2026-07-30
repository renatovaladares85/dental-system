use serde::Serialize;
use uuid::Uuid;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub correlation_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub field_errors: Vec<FieldError>,
}

impl AppError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
            correlation_id: Uuid::now_v7().to_string(),
            field_errors: Vec::new(),
        }
    }

    pub fn validation(field_errors: Vec<FieldError>) -> Self {
        Self {
            code: "VALIDATION_FAILED".to_owned(),
            message: "Revise os campos destacados e tente novamente.".to_owned(),
            correlation_id: Uuid::now_v7().to_string(),
            field_errors,
        }
    }

    pub fn conflict(message: &str) -> Self {
        Self::new("INVALID_STATE", message)
    }

    pub fn storage() -> Self {
        Self::new(
            "STORAGE_OPERATION_FAILED",
            "Não foi possível concluir a operação de armazenamento.",
        )
    }

    pub fn database() -> Self {
        Self::new(
            "DATABASE_OPERATION_FAILED",
            "Não foi possível concluir a operação no banco de dados.",
        )
    }

    pub fn security() -> Self {
        Self::new(
            "SECURITY_OPERATION_FAILED",
            "Não foi possível concluir a operação de segurança.",
        )
    }

    pub fn unauthenticated() -> Self {
        Self::new(
            "AUTHENTICATION_REQUIRED",
            "A sessão não é válida ou expirou.",
        )
    }

    pub fn invalid_credentials() -> Self {
        Self::new("INVALID_CREDENTIALS", "Usuário ou senha inválidos.")
    }

    pub fn forbidden(code: &str, message: &str) -> Self {
        Self::new(code, message)
    }

    pub fn rate_limited() -> Self {
        Self::new(
            "TOO_MANY_LOGIN_ATTEMPTS",
            "Aguarde alguns minutos antes de tentar novamente.",
        )
    }

    pub fn pairing_rate_limited() -> Self {
        Self::new(
            "TOO_MANY_PAIRING_ATTEMPTS",
            "Aguarde alguns minutos antes de tentar parear novamente.",
        )
    }

    #[cfg(not(windows))]
    pub fn unsupported_platform() -> Self {
        Self::new(
            "PLATFORM_KEY_PROTECTION_UNAVAILABLE",
            "A proteção da chave requer execução nativa no Windows.",
        )
    }

    pub fn worker() -> Self {
        Self::new(
            "BACKGROUND_OPERATION_FAILED",
            "A operação em segundo plano não pôde ser concluída.",
        )
    }
}

impl std::fmt::Debug for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppError")
            .field("code", &self.code)
            .field("correlation_id", &self.correlation_id)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({})", self.code, self.correlation_id)
    }
}

impl std::error::Error for AppError {}
