use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityDiagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlcipher_version: Option<String>,
    pub minimum_distribution_version: String,
    pub distribution_ready: bool,
    pub key_protection: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StartupState {
    Uninitialized {
        diagnostics: SecurityDiagnostics,
    },
    RecoveryPending {
        #[serde(rename = "setupId")]
        setup_id: String,
        stage: SetupStage,
        #[serde(rename = "completedStages")]
        completed_stages: Vec<SetupStage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        artifacts: Option<PendingArtifacts>,
        diagnostics: SecurityDiagnostics,
    },
    Ready {
        diagnostics: SecurityDiagnostics,
    },
    RecoveryRequired {
        #[serde(rename = "reasonCode")]
        reason_code: RecoveryReason,
        diagnostics: SecurityDiagnostics,
    },
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryReason {
    MissingKey,
    InvalidKey,
    DatabaseUnreadable,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStage {
    Database,
    MasterUser,
    RecoveryPackage,
    InitialBackup,
    Verification,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialSetupInput {
    pub organization: OrganizationInput,
    pub unit: UnitInput,
    pub master: MasterUserInput,
    pub storage: StorageInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationInput {
    pub name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitInput {
    pub name: String,
    pub responsible_name: String,
    pub phone: Option<String>,
    pub administrative_email: Option<String>,
    pub address: Option<String>,
    pub professional_registration: Option<String>,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct MasterUserInput {
    pub full_name: String,
    pub username: String,
    pub email: String,
    pub password: String,
    pub password_confirmation: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInput {
    pub backup_directory: String,
    pub recovery_package_directory: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmSetupInput {
    pub setup_id: String,
    pub recovery_code: String,
    pub acknowledged_separate_storage: bool,
    pub acknowledged_loss_risk: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgress {
    pub setup_id: String,
    pub stage: SetupStage,
    pub completed_stages: Vec<SetupStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<SetupArtifacts>,
}

#[derive(Clone, Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct SetupArtifacts {
    #[zeroize(skip)]
    pub recovery_package: ArtifactReference,
    #[zeroize(skip)]
    pub initial_backup: ArtifactReference,
    pub recovery_code: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingArtifacts {
    pub recovery_package: ArtifactReference,
    pub initial_backup: ArtifactReference,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactReference {
    pub path: String,
    pub file_name: String,
    pub sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedDirectory {
    pub path: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub id: String,
    pub full_name: String,
    pub username: String,
    pub email: String,
    pub roles: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<AuthUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_expires_at: Option<String>,
}

impl AuthSession {
    pub fn unauthenticated() -> Self {
        Self {
            authenticated: false,
            user: None,
            idle_expires_at: None,
            absolute_expires_at: None,
        }
    }
}

#[derive(Clone)]
pub struct LoginUserRecord {
    pub user: AuthUser,
    pub password_phc: zeroize::Zeroizing<String>,
}

#[derive(Clone)]
pub struct NewSessionRecord {
    pub id: String,
    pub user_id: String,
    pub token_hash: [u8; 32],
    pub csrf_hash: [u8; 32],
    pub created_at: String,
    pub idle_expires_at: String,
    pub absolute_expires_at: String,
    pub correlation_id: Option<String>,
    pub source: String,
}

#[derive(Clone)]
pub struct SessionRecord {
    pub id: String,
    pub user: AuthUser,
    pub csrf_hash: [u8; 32],
    pub idle_expires_at: String,
    pub absolute_expires_at: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: String,
    pub actor_type: String,
    pub actor_user_id: Option<String>,
    pub actor_username: Option<String>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub result: String,
    pub correlation_id: Option<String>,
    pub session_id: Option<String>,
    pub source: String,
    pub occurred_at: String,
}
