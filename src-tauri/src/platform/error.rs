use std::fmt;

pub type PlatformResult<T> = Result<T, PlatformError>;

/// Sanitized platform error. The code is safe to map to the HTTP error DTO.
/// OS error strings, paths and secret material deliberately are not retained.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PlatformError {
    code: &'static str,
}

impl PlatformError {
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub const fn code(self) -> &'static str {
        self.code
    }

    pub const fn invalid_input() -> Self {
        Self::new("PLATFORM_INVALID_INPUT")
    }

    pub const fn invalid_state() -> Self {
        Self::new("PLATFORM_INVALID_STATE")
    }

    pub const fn unavailable() -> Self {
        Self::new("PLATFORM_UNAVAILABLE")
    }

    pub const fn security() -> Self {
        Self::new("PLATFORM_SECURITY_FAILURE")
    }

    pub const fn storage() -> Self {
        Self::new("PLATFORM_STORAGE_FAILURE")
    }
}

impl fmt::Debug for PlatformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlatformError")
            .field("code", &self.code)
            .finish()
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for PlatformError {}
