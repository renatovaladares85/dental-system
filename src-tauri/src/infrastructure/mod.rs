mod artifacts;
mod database;
mod key_protection;

pub use artifacts::{create_artifact_pair, verify_artifact_pair};
pub use database::{
    ArtifactHistoryRecord, BootstrapRecord, CURRENT_SCHEMA_VERSION, CipherDiagnostics,
    DatabaseWorker, InstallationRecord, create_foundation_database, minimum_distribution_sqlcipher,
    runtime_security_diagnostics,
};
#[cfg(test)]
pub(crate) use key_protection::TestKeyProtector;
pub use key_protection::{KeyProtector, PlatformKeyProtector, ProtectedKeyFile};
