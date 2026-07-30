use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version, password_hash::SaltString};
use chrono::Utc;
use rand_core::{OsRng, RngCore};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    domain::{
        AppError, AppResult, ArtifactReference, ConfirmSetupInput, InitialSetupInput,
        PendingArtifacts, RecoveryReason, SecurityDiagnostics, SetupProgress, SetupStage,
        StartupState, StorageInput, validate_initial_setup,
    },
    infrastructure::{
        ArtifactHistoryRecord, BootstrapRecord, DatabaseWorker, KeyProtector, PlatformKeyProtector,
        ProtectedKeyFile, create_artifact_pair, create_foundation_database,
        minimum_distribution_sqlcipher, runtime_security_diagnostics, verify_artifact_pair,
    },
};

const ACTIVE_DIRECTORY: &str = "active";
const DATABASE_FILE: &str = "database.sqlcipher";
const PROTECTED_KEY_FILE: &str = "database-key.dpapi.json";

pub struct SetupService {
    app_data_directory: PathBuf,
    active_directory: PathBuf,
    installation_id: String,
    database: Arc<DatabaseWorker>,
    key_protector: Arc<dyn KeyProtector>,
    operation_lock: Mutex<()>,
}

impl SetupService {
    pub fn new(app_data_directory: PathBuf) -> Self {
        Self::for_installation(app_data_directory, Uuid::now_v7())
    }

    pub fn for_installation(app_data_directory: PathBuf, installation_id: Uuid) -> Self {
        Self::with_key_protector_and_id(
            app_data_directory,
            installation_id.to_string(),
            Arc::new(PlatformKeyProtector::new()),
        )
    }

    #[cfg(test)]
    fn with_key_protector(
        app_data_directory: PathBuf,
        key_protector: Arc<dyn KeyProtector>,
    ) -> Self {
        Self::with_key_protector_and_id(
            app_data_directory,
            Uuid::now_v7().to_string(),
            key_protector,
        )
    }

    fn with_key_protector_and_id(
        app_data_directory: PathBuf,
        installation_id: String,
        key_protector: Arc<dyn KeyProtector>,
    ) -> Self {
        let active_directory = app_data_directory.join(ACTIVE_DIRECTORY);
        let database_path = active_directory.join(DATABASE_FILE);
        Self {
            app_data_directory,
            active_directory,
            installation_id,
            database: Arc::new(DatabaseWorker::new(database_path)),
            key_protector,
            operation_lock: Mutex::new(()),
        }
    }

    pub fn database_worker(&self) -> Arc<DatabaseWorker> {
        self.database.clone()
    }

    pub fn active_directory(&self) -> &Path {
        &self.active_directory
    }

    pub fn get_startup_state(&self) -> AppResult<StartupState> {
        let _operation = self.operation_lock.lock().map_err(|_| AppError::worker())?;
        self.startup_state_unlocked()
    }

    pub fn start_initial_setup(&self, input: InitialSetupInput) -> AppResult<SetupProgress> {
        let _operation = self.operation_lock.lock().map_err(|_| AppError::worker())?;
        if self.active_directory.exists() {
            return Err(AppError::conflict(
                "A configuração inicial já foi iniciada nesta instalação.",
            ));
        }

        fs::create_dir_all(&self.app_data_directory).map_err(|_| AppError::storage())?;
        let mut validated = validate_initial_setup(input)?;
        let (recovery_directory, backup_directory) = self
            .validate_storage_paths(&validated.recovery_directory, &validated.backup_directory)?;
        validated.recovery_directory = recovery_directory;
        validated.backup_directory = backup_directory;

        let setup_id = self.installation_id.clone();
        let database_id = Uuid::now_v7().to_string();
        let key_id = Uuid::now_v7().to_string();
        let mut database_key = Zeroizing::new([0_u8; 32]);
        OsRng.fill_bytes(database_key.as_mut());
        let password_phc = hash_password(validated.master_password.as_bytes())?;
        let protected_key = self.key_protector.protect(database_key.as_ref())?;
        let protected_key_file = ProtectedKeyFile::new(
            database_id.clone(),
            key_id.clone(),
            self.key_protector.protection_name(),
            &protected_key,
        );

        let staging_directory = self
            .app_data_directory
            .join(format!(".setup-{setup_id}.partial"));
        quarantine_stale_staging(&staging_directory, &self.app_data_directory, &setup_id)?;
        fs::create_dir(&staging_directory).map_err(|_| AppError::storage())?;
        let staged_database_path = staging_directory.join(DATABASE_FILE);
        let staged_key_path = staging_directory.join(PROTECTED_KEY_FILE);
        let now = now();
        let bootstrap = BootstrapRecord {
            setup_id: setup_id.clone(),
            database_id: database_id.clone(),
            key_id: key_id.clone(),
            organization_id: Uuid::now_v7().to_string(),
            unit_id: Uuid::now_v7().to_string(),
            master_user_id: Uuid::now_v7().to_string(),
            master_role_id: Uuid::now_v7().to_string(),
            organization_name: validated.organization_name,
            unit_name: validated.unit_name,
            responsible_name: validated.responsible_name,
            phone: validated.phone,
            administrative_email: validated.administrative_email,
            address: validated.address,
            professional_registration: validated.professional_registration,
            master_full_name: validated.master_full_name,
            master_username: validated.master_username,
            master_email: validated.master_email,
            password_phc,
            backup_directory: validated.backup_directory.to_string_lossy().into_owned(),
            recovery_directory: validated.recovery_directory.to_string_lossy().into_owned(),
            now,
        };

        let stage_result = (|| {
            create_foundation_database(&staged_database_path, database_key.as_ref(), &bootstrap)?;
            write_new_synced(&staged_key_path, &protected_key_file.to_bytes()?)?;
            crate::platform::sync_directory(&staging_directory).map_err(|_| AppError::storage())?;
            if self.active_directory.exists() {
                return Err(AppError::conflict(
                    "Outra configuração inicial foi iniciada simultaneamente.",
                ));
            }
            fs::rename(&staging_directory, &self.active_directory)
                .map_err(|_| AppError::storage())?;
            crate::platform::sync_directory(&self.app_data_directory)
                .map_err(|_| AppError::storage())?;
            Ok(())
        })();
        if stage_result.is_err() && staging_directory.exists() {
            let _ = fs::remove_dir_all(&staging_directory);
        }
        stage_result?;

        self.database.open(database_key.as_ref())?;
        self.database.verify_identity(&database_id, &key_id)?;
        self.generate_and_record_artifacts(
            &setup_id,
            &database_id,
            &key_id,
            &validated.recovery_directory,
            &validated.backup_directory,
            database_key.as_ref(),
        )
    }

    pub fn resume_initial_setup(
        &self,
        setup_id: String,
        storage: Option<StorageInput>,
    ) -> AppResult<SetupProgress> {
        let _operation = self.operation_lock.lock().map_err(|_| AppError::worker())?;
        let (key_file, database_key) = self.open_existing_database()?;
        let mut installation = self.database.installation()?;
        if installation.database_id != key_file.database_id
            || installation.key_id != key_file.key_id
            || installation.setup_id != self.installation_id
        {
            return Err(AppError::security());
        }
        if installation.setup_id != setup_id || installation.setup_state != "RECOVERY_PENDING" {
            return Err(AppError::conflict(
                "A configuração informada não está aguardando recuperação.",
            ));
        }

        if let Some(storage) = storage {
            let (recovery_directory, backup_directory) = self.validate_storage_paths(
                Path::new(storage.recovery_package_directory.trim()),
                Path::new(storage.backup_directory.trim()),
            )?;
            self.database.update_storage(
                &setup_id,
                &backup_directory.to_string_lossy(),
                &recovery_directory.to_string_lossy(),
                &now(),
            )?;
            installation.backup_directory = backup_directory;
            installation.recovery_directory = recovery_directory;
        }

        self.generate_and_record_artifacts(
            &setup_id,
            &key_file.database_id,
            &key_file.key_id,
            &installation.recovery_directory,
            &installation.backup_directory,
            database_key.as_ref(),
        )
    }

    pub fn confirm_initial_setup(&self, mut input: ConfirmSetupInput) -> AppResult<StartupState> {
        let recovery_code = Zeroizing::new(std::mem::take(&mut input.recovery_code));
        let _operation = self.operation_lock.lock().map_err(|_| AppError::worker())?;

        let mut field_errors = Vec::new();
        if !input.acknowledged_separate_storage {
            field_errors.push(crate::domain::FieldError {
                field: "acknowledgedSeparateStorage".to_owned(),
                message: "Confirme que o código será guardado separado do pacote.".to_owned(),
            });
        }
        if !input.acknowledged_loss_risk {
            field_errors.push(crate::domain::FieldError {
                field: "acknowledgedLossRisk".to_owned(),
                message: "Confirme que compreende o risco de perda dos artefatos.".to_owned(),
            });
        }
        if !field_errors.is_empty() {
            return Err(AppError::validation(field_errors));
        }

        let (key_file, database_key) = self.open_existing_database()?;
        let installation = self.database.installation()?;
        if installation.database_id != key_file.database_id
            || installation.key_id != key_file.key_id
            || installation.setup_id != self.installation_id
        {
            return Err(AppError::security());
        }
        if installation.setup_id != input.setup_id || installation.setup_state != "RECOVERY_PENDING"
        {
            return Err(AppError::conflict(
                "A configuração informada não está aguardando confirmação.",
            ));
        }
        let recovery = installation.recovery_package.ok_or_else(|| {
            AppError::conflict("O pacote de recuperação inicial ainda não foi criado.")
        })?;
        let backup = installation
            .initial_backup
            .ok_or_else(|| AppError::conflict("O backup inicial ainda não foi criado."))?;

        verify_artifact_pair(
            &recovery,
            &backup,
            &installation.recovery_directory,
            &installation.backup_directory,
            &recovery_code,
            database_key.as_ref(),
            &installation.setup_id,
            &key_file.database_id,
            &key_file.key_id,
        )?;
        self.database
            .mark_ready(&installation.setup_id, &recovery.id, &backup.id, &now())?;
        let diagnostics = self.database.diagnostics()?;
        Ok(StartupState::Ready {
            diagnostics: self.security_diagnostics(Some(diagnostics)),
        })
    }

    fn startup_state_unlocked(&self) -> AppResult<StartupState> {
        let diagnostics = self.security_diagnostics(None);
        if !self.active_directory.exists() {
            return Ok(StartupState::Uninitialized { diagnostics });
        }
        let database_path = self.active_directory.join(DATABASE_FILE);
        let key_path = self.active_directory.join(PROTECTED_KEY_FILE);
        if !database_path.is_file() {
            return Ok(StartupState::RecoveryRequired {
                reason_code: RecoveryReason::DatabaseUnreadable,
                diagnostics,
            });
        }
        if !key_path.is_file() {
            return Ok(StartupState::RecoveryRequired {
                reason_code: RecoveryReason::MissingKey,
                diagnostics,
            });
        }

        let key_file = match ProtectedKeyFile::read(&key_path) {
            Ok(value) => value,
            Err(_) => {
                return Ok(StartupState::RecoveryRequired {
                    reason_code: RecoveryReason::InvalidKey,
                    diagnostics,
                });
            }
        };
        if key_file.protection != self.key_protector.protection_name() {
            return Ok(StartupState::RecoveryRequired {
                reason_code: RecoveryReason::InvalidKey,
                diagnostics,
            });
        }
        let protected_blob = match key_file.decode_blob() {
            Ok(value) => value,
            Err(_) => {
                return Ok(StartupState::RecoveryRequired {
                    reason_code: RecoveryReason::InvalidKey,
                    diagnostics,
                });
            }
        };
        let database_key = match self.key_protector.unprotect(&protected_blob) {
            Ok(value) if value.len() == 32 => value,
            _ => {
                return Ok(StartupState::RecoveryRequired {
                    reason_code: RecoveryReason::InvalidKey,
                    diagnostics,
                });
            }
        };
        let cipher = match self.database.open(database_key.as_ref()) {
            Ok(value) => value,
            Err(_) => {
                return Ok(StartupState::RecoveryRequired {
                    reason_code: RecoveryReason::DatabaseUnreadable,
                    diagnostics,
                });
            }
        };
        if self
            .database
            .verify_identity(&key_file.database_id, &key_file.key_id)
            .is_err()
        {
            return Ok(StartupState::RecoveryRequired {
                reason_code: RecoveryReason::InvalidKey,
                diagnostics: self.security_diagnostics(Some(cipher)),
            });
        }
        let installation = match self.database.installation() {
            Ok(value) => value,
            Err(_) => {
                return Ok(StartupState::RecoveryRequired {
                    reason_code: RecoveryReason::DatabaseUnreadable,
                    diagnostics: self.security_diagnostics(Some(cipher)),
                });
            }
        };
        if installation.database_id != key_file.database_id
            || installation.key_id != key_file.key_id
            || installation.setup_id != self.installation_id
        {
            return Ok(StartupState::RecoveryRequired {
                reason_code: RecoveryReason::InvalidKey,
                diagnostics: self.security_diagnostics(Some(cipher)),
            });
        }
        let diagnostics = self.security_diagnostics(Some(cipher));

        match installation.setup_state.as_str() {
            "READY" => Ok(StartupState::Ready { diagnostics }),
            "RECOVERY_PENDING" => {
                let artifacts = pending_artifacts(&installation);
                let completed_stages = if artifacts.is_some() {
                    vec![
                        SetupStage::Database,
                        SetupStage::MasterUser,
                        SetupStage::RecoveryPackage,
                        SetupStage::InitialBackup,
                    ]
                } else {
                    vec![SetupStage::Database, SetupStage::MasterUser]
                };
                let stage = if artifacts.is_some() {
                    SetupStage::Verification
                } else {
                    SetupStage::RecoveryPackage
                };
                Ok(StartupState::RecoveryPending {
                    setup_id: installation.setup_id,
                    stage,
                    completed_stages,
                    artifacts,
                    diagnostics,
                })
            }
            _ => Ok(StartupState::RecoveryRequired {
                reason_code: RecoveryReason::DatabaseUnreadable,
                diagnostics,
            }),
        }
    }

    fn open_existing_database(&self) -> AppResult<(ProtectedKeyFile, Zeroizing<Vec<u8>>)> {
        let key_path = self.active_directory.join(PROTECTED_KEY_FILE);
        let key_file = ProtectedKeyFile::read(&key_path)?;
        if key_file.protection != self.key_protector.protection_name() {
            return Err(AppError::security());
        }
        let protected_blob = key_file.decode_blob()?;
        let database_key = self.key_protector.unprotect(&protected_blob)?;
        if database_key.len() != 32 {
            return Err(AppError::security());
        }
        self.database.open(database_key.as_ref())?;
        self.database
            .verify_identity(&key_file.database_id, &key_file.key_id)?;
        Ok((key_file, database_key))
    }

    fn generate_and_record_artifacts(
        &self,
        setup_id: &str,
        database_id: &str,
        key_id: &str,
        recovery_directory: &Path,
        backup_directory: &Path,
        database_key: &[u8],
    ) -> AppResult<SetupProgress> {
        let diagnostics = self.database.diagnostics()?;
        let pair = create_artifact_pair(
            &self.database,
            database_key,
            setup_id,
            database_id,
            key_id,
            recovery_directory,
            backup_directory,
            &diagnostics.version,
        )?;
        self.database
            .record_artifacts(setup_id, &pair.recovery, &pair.backup)?;
        Ok(progress_with_artifacts(setup_id.to_owned(), pair.public))
    }

    fn validate_storage_paths(
        &self,
        recovery_path: &Path,
        backup_path: &Path,
    ) -> AppResult<(PathBuf, PathBuf)> {
        let recovery = canonical_directory(recovery_path, "storage.recoveryPackageDirectory")?;
        let backup = canonical_directory(backup_path, "storage.backupDirectory")?;
        let app_data =
            fs::canonicalize(&self.app_data_directory).map_err(|_| AppError::storage())?;
        if recovery == backup
            || recovery.starts_with(&backup)
            || backup.starts_with(&recovery)
            || recovery.starts_with(&app_data)
            || backup.starts_with(&app_data)
        {
            return Err(AppError::validation(vec![crate::domain::FieldError {
                field: "storage".to_owned(),
                message: "Escolha locais existentes, distintos e externos aos dados do aplicativo."
                    .to_owned(),
            }]));
        }
        Ok((recovery, backup))
    }

    fn security_diagnostics(
        &self,
        cipher: Option<crate::infrastructure::CipherDiagnostics>,
    ) -> SecurityDiagnostics {
        let runtime = cipher
            .is_none()
            .then(runtime_security_diagnostics)
            .and_then(Result::ok);
        SecurityDiagnostics {
            sqlcipher_version: cipher
                .as_ref()
                .map(|value| value.version.clone())
                .or_else(|| {
                    runtime
                        .as_ref()
                        .map(|value| value.sqlcipher_version.clone())
                }),
            minimum_distribution_version: minimum_distribution_sqlcipher().to_owned(),
            distribution_ready: cipher
                .as_ref()
                .is_some_and(|value| value.distribution_ready)
                || runtime.is_some_and(|value| value.distribution_ready),
            key_protection: self.key_protector.protection_name().to_owned(),
        }
    }
}

fn canonical_directory(path: &Path, field: &str) -> AppResult<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|_| {
        AppError::validation(vec![crate::domain::FieldError {
            field: field.to_owned(),
            message: "Selecione um diretório existente e acessível.".to_owned(),
        }])
    })?;
    if !canonical.is_dir() {
        return Err(AppError::validation(vec![crate::domain::FieldError {
            field: field.to_owned(),
            message: "Selecione um diretório existente e acessível.".to_owned(),
        }]));
    }
    Ok(canonical)
}

fn hash_password(password: &[u8]) -> AppResult<Zeroizing<String>> {
    let mut salt = [0_u8; 16];
    OsRng.fill_bytes(&mut salt);
    let salt_string = SaltString::encode_b64(&salt).map_err(|_| AppError::security())?;
    salt.zeroize();
    let parameters = Params::new(65_536, 3, 1, Some(32)).map_err(|_| AppError::security())?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, parameters);
    argon
        .hash_password(password, &salt_string)
        .map(|hash| Zeroizing::new(hash.to_string()))
        .map_err(|_| AppError::security())
}

fn pending_artifacts(
    installation: &crate::infrastructure::InstallationRecord,
) -> Option<PendingArtifacts> {
    match (
        installation.recovery_package.as_ref(),
        installation.initial_backup.as_ref(),
    ) {
        (Some(recovery), Some(backup)) => Some(PendingArtifacts {
            recovery_package: artifact_reference(recovery),
            initial_backup: artifact_reference(backup),
        }),
        _ => None,
    }
}

fn artifact_reference(record: &ArtifactHistoryRecord) -> ArtifactReference {
    ArtifactReference {
        path: record.path.clone(),
        file_name: record.file_name.clone(),
        sha256: record.sha256.clone(),
    }
}

fn progress_with_artifacts(
    setup_id: String,
    artifacts: crate::domain::SetupArtifacts,
) -> SetupProgress {
    SetupProgress {
        setup_id,
        stage: SetupStage::Verification,
        completed_stages: vec![
            SetupStage::Database,
            SetupStage::MasterUser,
            SetupStage::RecoveryPackage,
            SetupStage::InitialBackup,
        ],
        artifacts: Some(artifacts),
    }
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| AppError::storage())?;
    file.write_all(bytes).map_err(|_| AppError::storage())?;
    file.flush().map_err(|_| AppError::storage())?;
    file.sync_all().map_err(|_| AppError::storage())
}

fn quarantine_stale_staging(
    staging: &Path,
    app_data_directory: &Path,
    setup_id: &str,
) -> AppResult<()> {
    let metadata = match fs::symlink_metadata(staging) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(AppError::storage()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(AppError::security());
    }
    let quarantine =
        app_data_directory.join(format!(".abandoned-setup-{setup_id}-{}", Uuid::now_v7()));
    fs::rename(staging, quarantine).map_err(|_| AppError::storage())?;
    crate::platform::sync_directory(app_data_directory).map_err(|_| AppError::storage())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{MasterUserInput, OrganizationInput, StorageInput, UnitInput},
        infrastructure::TestKeyProtector,
    };
    use std::io::{Read as _, Seek, SeekFrom};
    use tempfile::tempdir;

    #[test]
    fn uninitialized_reports_linked_sqlcipher_diagnostics() {
        let app = tempdir().expect("app data");
        let service =
            SetupService::with_key_protector(app.path().to_path_buf(), Arc::new(TestKeyProtector));

        let StartupState::Uninitialized { diagnostics } =
            service.get_startup_state().expect("uninitialized state")
        else {
            panic!("expected uninitialized state");
        };
        assert!(diagnostics.distribution_ready);
        assert!(diagnostics.sqlcipher_version.is_some());
    }

    #[test]
    fn starts_pending_and_only_becomes_ready_after_verification() {
        let app = tempdir().expect("app data");
        let recovery = tempdir().expect("recovery");
        let backup = tempdir().expect("backup");
        let service =
            SetupService::with_key_protector(app.path().to_path_buf(), Arc::new(TestKeyProtector));
        let progress = service
            .start_initial_setup(input(recovery.path(), backup.path()))
            .expect("start setup");
        assert!(matches!(
            service.get_startup_state().expect("state"),
            StartupState::RecoveryPending { .. }
        ));
        let code = progress
            .artifacts
            .as_ref()
            .expect("artifacts")
            .recovery_code
            .clone();
        let ready = service
            .confirm_initial_setup(ConfirmSetupInput {
                setup_id: progress.setup_id,
                recovery_code: code,
                acknowledged_separate_storage: true,
                acknowledged_loss_risk: true,
            })
            .expect("confirm");
        assert!(matches!(ready, StartupState::Ready { .. }));
    }

    #[test]
    fn corrupted_backup_keeps_setup_pending() {
        let app = tempdir().expect("app data");
        let recovery = tempdir().expect("recovery");
        let backup = tempdir().expect("backup");
        let service =
            SetupService::with_key_protector(app.path().to_path_buf(), Arc::new(TestKeyProtector));
        let progress = service
            .start_initial_setup(input(recovery.path(), backup.path()))
            .expect("start setup");
        let artifacts = progress.artifacts.as_ref().expect("artifacts");
        let code = artifacts.recovery_code.clone();
        let mut backup_file = std::fs::File::options()
            .read(true)
            .write(true)
            .open(&artifacts.initial_backup.path)
            .expect("open backup");
        backup_file.seek(SeekFrom::End(-1)).expect("seek");
        let mut last = [0_u8; 1];
        backup_file.read_exact(&mut last).expect("read");
        backup_file.seek(SeekFrom::End(-1)).expect("seek again");
        backup_file
            .write_all(&[last[0] ^ 0xff])
            .expect("corrupt backup");
        backup_file.sync_all().expect("sync corruption");

        let result = service.confirm_initial_setup(ConfirmSetupInput {
            setup_id: progress.setup_id,
            recovery_code: code,
            acknowledged_separate_storage: true,
            acknowledged_loss_risk: true,
        });
        assert!(result.is_err());
        assert!(matches!(
            service.get_startup_state().expect("state"),
            StartupState::RecoveryPending { .. }
        ));
    }

    #[test]
    fn restart_can_resume_in_new_directories_with_a_new_code() {
        let app = tempdir().expect("app data");
        let recovery = tempdir().expect("recovery");
        let backup = tempdir().expect("backup");
        let installation_id = Uuid::now_v7().to_string();
        let service = SetupService::with_key_protector_and_id(
            app.path().to_path_buf(),
            installation_id.clone(),
            Arc::new(TestKeyProtector),
        );
        let first = service
            .start_initial_setup(input(recovery.path(), backup.path()))
            .expect("start setup");
        let setup_id = first.setup_id.clone();
        let first_code = first
            .artifacts
            .as_ref()
            .expect("first artifacts")
            .recovery_code
            .clone();
        drop(service);

        let new_recovery = tempdir().expect("new recovery");
        let new_backup = tempdir().expect("new backup");
        let restarted = SetupService::with_key_protector_and_id(
            app.path().to_path_buf(),
            installation_id,
            Arc::new(TestKeyProtector),
        );
        assert!(matches!(
            restarted
                .get_startup_state()
                .expect("pending after restart"),
            StartupState::RecoveryPending { .. }
        ));
        let resumed = restarted
            .resume_initial_setup(
                setup_id.clone(),
                Some(StorageInput {
                    backup_directory: new_backup.path().to_string_lossy().into_owned(),
                    recovery_package_directory: new_recovery.path().to_string_lossy().into_owned(),
                }),
            )
            .expect("resume");
        let resumed_artifacts = resumed.artifacts.as_ref().expect("resumed artifacts");
        assert_ne!(resumed_artifacts.recovery_code, first_code);
        let canonical_recovery = fs::canonicalize(new_recovery.path()).expect("canonical recovery");
        let canonical_backup = fs::canonicalize(new_backup.path()).expect("canonical backup");
        assert_eq!(
            Path::new(&resumed_artifacts.recovery_package.path).parent(),
            Some(canonical_recovery.as_path())
        );
        assert_eq!(
            Path::new(&resumed_artifacts.initial_backup.path).parent(),
            Some(canonical_backup.as_path())
        );

        let ready = restarted
            .confirm_initial_setup(ConfirmSetupInput {
                setup_id,
                recovery_code: resumed_artifacts.recovery_code.clone(),
                acknowledged_separate_storage: true,
                acknowledged_loss_risk: true,
            })
            .expect("confirm resumed setup");
        assert!(matches!(ready, StartupState::Ready { .. }));
    }

    #[test]
    fn tampered_key_protection_identifier_requires_recovery() {
        let app = tempdir().expect("app data");
        let recovery = tempdir().expect("recovery");
        let backup = tempdir().expect("backup");
        let service =
            SetupService::with_key_protector(app.path().to_path_buf(), Arc::new(TestKeyProtector));
        service
            .start_initial_setup(input(recovery.path(), backup.path()))
            .expect("start setup");
        let key_path = app.path().join(ACTIVE_DIRECTORY).join(PROTECTED_KEY_FILE);
        let mut key_file = ProtectedKeyFile::read(&key_path).expect("key file");
        key_file.protection = "tampered-protection".to_owned();
        std::fs::write(&key_path, key_file.to_bytes().expect("key bytes"))
            .expect("tamper key metadata");

        assert!(matches!(
            service.get_startup_state().expect("state"),
            StartupState::RecoveryRequired {
                reason_code: RecoveryReason::InvalidKey,
                ..
            }
        ));
    }

    #[test]
    fn database_bound_to_another_host_installation_requires_recovery() {
        let app = tempdir().expect("app data");
        let recovery = tempdir().expect("recovery");
        let backup = tempdir().expect("backup");
        let first = SetupService::with_key_protector_and_id(
            app.path().to_path_buf(),
            Uuid::now_v7().to_string(),
            Arc::new(TestKeyProtector),
        );
        first
            .start_initial_setup(input(recovery.path(), backup.path()))
            .expect("start setup");
        drop(first);

        let mismatched = SetupService::with_key_protector_and_id(
            app.path().to_path_buf(),
            Uuid::now_v7().to_string(),
            Arc::new(TestKeyProtector),
        );
        assert!(matches!(
            mismatched.get_startup_state().expect("state"),
            StartupState::RecoveryRequired {
                reason_code: RecoveryReason::InvalidKey,
                ..
            }
        ));
    }

    #[test]
    fn stale_setup_staging_is_quarantined_before_a_safe_retry() {
        let app = tempdir().expect("app data");
        let recovery = tempdir().expect("recovery");
        let backup = tempdir().expect("backup");
        let installation_id = Uuid::now_v7().to_string();
        let stale = app.path().join(format!(".setup-{installation_id}.partial"));
        fs::create_dir(&stale).expect("stale staging");
        fs::write(stale.join("interrupted.marker"), b"incomplete").expect("stale marker");
        let service = SetupService::with_key_protector_and_id(
            app.path().to_path_buf(),
            installation_id.clone(),
            Arc::new(TestKeyProtector),
        );

        service
            .start_initial_setup(input(recovery.path(), backup.path()))
            .expect("retry setup");

        assert!(app.path().join(ACTIVE_DIRECTORY).is_dir());
        assert!(!stale.exists());
        let quarantined = fs::read_dir(app.path())
            .expect("list app data")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&format!(".abandoned-setup-{installation_id}-"))
                    })
            })
            .expect("quarantined staging");
        assert_eq!(
            fs::read(quarantined.join("interrupted.marker")).expect("quarantined marker"),
            b"incomplete"
        );
    }

    #[test]
    fn concurrent_initial_setup_has_exactly_one_winner() {
        let app = tempdir().expect("app data");
        let recovery = tempdir().expect("recovery");
        let backup = tempdir().expect("backup");
        let service = Arc::new(SetupService::with_key_protector(
            app.path().to_path_buf(),
            Arc::new(TestKeyProtector),
        ));
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let handles = (0..2)
            .map(|_| {
                let service = service.clone();
                let barrier = barrier.clone();
                let recovery = recovery.path().to_path_buf();
                let backup = backup.path().to_path_buf();
                std::thread::spawn(move || {
                    barrier.wait();
                    service.start_initial_setup(input(&recovery, &backup))
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("setup thread"))
            .collect::<Vec<_>>();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    }

    fn input(recovery: &Path, backup: &Path) -> InitialSetupInput {
        InitialSetupInput {
            organization: OrganizationInput {
                name: "Clínica Horizonte".to_owned(),
            },
            unit: UnitInput {
                name: "Unidade principal".to_owned(),
                responsible_name: "Ana Souza".to_owned(),
                phone: None,
                administrative_email: None,
                address: None,
                professional_registration: None,
            },
            master: MasterUserInput {
                full_name: "Carlos Oliveira".to_owned(),
                username: "carlos.admin".to_owned(),
                email: "carlos@example.test".to_owned(),
                password: "frase longa e exclusiva 2026".to_owned(),
                password_confirmation: "frase longa e exclusiva 2026".to_owned(),
            },
            storage: StorageInput {
                backup_directory: backup.to_string_lossy().into_owned(),
                recovery_package_directory: recovery.to_string_lossy().into_owned(),
            },
        }
    }
}
