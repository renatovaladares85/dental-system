use std::{
    fmt::Write as _,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, backup::Backup, params};
use semver::Version;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{
    application::SessionStore,
    domain::{
        AppError, AppResult, AuditEvent, AuthUser, LoginUserRecord, NewSessionRecord, SessionRecord,
    },
};

const FOUNDATION_MIGRATION: &str = include_str!("../../migrations/0001_foundation.sql");
const WEB_IDENTITY_SESSIONS_MIGRATION: &str =
    include_str!("../../migrations/0002_web_identity_sessions.sql");
const OPERATIONAL_AUDIT_MIGRATION: &str =
    include_str!("../../migrations/0003_operational_audit.sql");
const MINIMUM_DISTRIBUTION_SQLCIPHER: &str = "4.17.0";
pub const CURRENT_SCHEMA_VERSION: u32 = 3;

#[derive(Clone)]
pub struct CipherDiagnostics {
    pub version: String,
    pub distribution_ready: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSecurityDiagnostics {
    pub sqlcipher_version: String,
    pub minimum_distribution_version: &'static str,
    pub distribution_ready: bool,
}

pub struct BootstrapRecord {
    pub setup_id: String,
    pub database_id: String,
    pub key_id: String,
    pub organization_id: String,
    pub unit_id: String,
    pub master_user_id: String,
    pub master_role_id: String,
    pub organization_name: String,
    pub unit_name: String,
    pub responsible_name: String,
    pub phone: Option<String>,
    pub administrative_email: Option<String>,
    pub address: Option<String>,
    pub professional_registration: Option<String>,
    pub master_full_name: String,
    pub master_username: String,
    pub master_email: String,
    pub password_phc: Zeroizing<String>,
    pub backup_directory: String,
    pub recovery_directory: String,
    pub now: String,
}

#[derive(Clone)]
pub struct ArtifactHistoryRecord {
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub sha256: String,
    pub database_sha256: Option<String>,
    pub created_at: String,
}

pub struct InstallationRecord {
    pub setup_id: String,
    pub database_id: String,
    pub key_id: String,
    pub setup_state: String,
    pub backup_directory: PathBuf,
    pub recovery_directory: PathBuf,
    pub recovery_package: Option<ArtifactHistoryRecord>,
    pub initial_backup: Option<ArtifactHistoryRecord>,
}

pub struct DatabaseWorker {
    path: PathBuf,
    connection: Mutex<Option<Connection>>,
}

impl SessionStore for DatabaseWorker {
    fn find_login_user(&self, username: &str) -> AppResult<Option<LoginUserRecord>> {
        DatabaseWorker::find_login_user(self, username)
    }

    fn create_session(&self, session: &NewSessionRecord) -> AppResult<()> {
        DatabaseWorker::create_session(self, session)
    }

    fn find_active_session(
        &self,
        token_hash: &[u8; 32],
        now: &str,
    ) -> AppResult<Option<SessionRecord>> {
        DatabaseWorker::find_active_session(self, token_hash, now)
    }

    fn touch_session(
        &self,
        session_id: &str,
        token_hash: &[u8; 32],
        now: &str,
        idle_expires_at: &str,
    ) -> AppResult<()> {
        DatabaseWorker::touch_session(self, session_id, token_hash, now, idle_expires_at)
    }

    fn rotate_session(
        &self,
        session_id: &str,
        old_token_hash: &[u8; 32],
        new_token_hash: &[u8; 32],
        new_csrf_hash: &[u8; 32],
        now: &str,
        idle_expires_at: &str,
        correlation_id: Option<&str>,
        source: &str,
    ) -> AppResult<()> {
        DatabaseWorker::rotate_session(
            self,
            session_id,
            old_token_hash,
            new_token_hash,
            new_csrf_hash,
            now,
            idle_expires_at,
            correlation_id,
            source,
        )
    }

    fn revoke_session(
        &self,
        session_id: &str,
        token_hash: &[u8; 32],
        user_id: &str,
        now: &str,
        correlation_id: Option<&str>,
        source: &str,
    ) -> AppResult<()> {
        DatabaseWorker::revoke_session(
            self,
            session_id,
            token_hash,
            user_id,
            now,
            correlation_id,
            source,
        )
    }

    fn list_audit_events(
        &self,
        user_id: &str,
        session_id: &str,
        correlation_id: &str,
        limit: u32,
    ) -> AppResult<Vec<AuditEvent>> {
        DatabaseWorker::list_audit_events(self, user_id, session_id, correlation_id, limit)
    }
}

impl DatabaseWorker {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            connection: Mutex::new(None),
        }
    }

    pub fn open(&self, key: &[u8]) -> AppResult<CipherDiagnostics> {
        let mut guard = self.connection.lock().map_err(|_| AppError::database())?;
        if guard.is_none() {
            let connection = open_encrypted_connection(&self.path, key, false, true)?;
            migrate_database(&connection)?;
            validate_connection(&connection)?;
            *guard = Some(connection);
        }
        cipher_diagnostics(guard.as_ref().ok_or_else(AppError::database)?)
    }

    pub fn installation(&self) -> AppResult<InstallationRecord> {
        self.with_connection(|connection| load_installation(connection))
    }

    pub fn verify_identity(&self, database_id: &str, key_id: &str) -> AppResult<()> {
        self.with_connection(|connection| {
            let matches: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM installations WHERE database_id = ?1 AND key_id = ?2",
                    params![database_id, key_id],
                    |row| row.get(0),
                )
                .map_err(|_| AppError::database())?;
            if matches != 1 {
                return Err(AppError::security());
            }
            Ok(())
        })
    }

    pub fn record_artifacts(
        &self,
        setup_id: &str,
        recovery: &ArtifactHistoryRecord,
        backup: &ArtifactHistoryRecord,
    ) -> AppResult<()> {
        self.with_connection(|connection| {
            let transaction = connection
                .transaction()
                .map_err(|_| AppError::database())?;
            ensure_pending(&transaction, setup_id)?;
            transaction
                .execute(
                    "INSERT INTO recovery_package_history
                     (id, installation_id, file_path, file_name, sha256, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        recovery.id,
                        setup_id,
                        recovery.path,
                        recovery.file_name,
                        recovery.sha256,
                        recovery.created_at,
                    ],
                )
                .map_err(|_| AppError::database())?;
            transaction
                .execute(
                    "INSERT INTO backup_history
                     (id, installation_id, file_path, file_name, sha256, database_sha256, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        backup.id,
                        setup_id,
                        backup.path,
                        backup.file_name,
                        backup.sha256,
                        backup.database_sha256,
                        backup.created_at,
                    ],
                )
                .map_err(|_| AppError::database())?;
            let settings_updated = transaction
                .execute(
                    "UPDATE installation_settings
                     SET current_recovery_package_id = ?1, current_backup_id = ?2, updated_at = ?3
                     WHERE installation_id = ?4",
                    params![recovery.id, backup.id, backup.created_at, setup_id],
                )
                .map_err(|_| AppError::database())?;
            if settings_updated != 1 {
                return Err(AppError::database());
            }
            insert_audit(
                &transaction,
                setup_id,
                "INITIAL_RECOVERY_ARTIFACTS_CREATED",
                "installation",
                Some(setup_id),
                &backup.created_at,
            )?;
            transaction.commit().map_err(|_| AppError::database())
        })
    }

    pub fn update_storage(
        &self,
        setup_id: &str,
        backup_directory: &str,
        recovery_directory: &str,
        now: &str,
    ) -> AppResult<()> {
        self.with_connection(|connection| {
            let transaction = connection.transaction().map_err(|_| AppError::database())?;
            ensure_pending(&transaction, setup_id)?;
            let updated = transaction
                .execute(
                    "UPDATE installation_settings
                     SET backup_directory = ?1, recovery_directory = ?2, updated_at = ?3
                     WHERE installation_id = ?4",
                    params![backup_directory, recovery_directory, now, setup_id],
                )
                .map_err(|_| AppError::database())?;
            if updated != 1 {
                return Err(AppError::database());
            }
            insert_audit(
                &transaction,
                setup_id,
                "INITIAL_SETUP_STORAGE_CHANGED",
                "installation",
                Some(setup_id),
                now,
            )?;
            transaction.commit().map_err(|_| AppError::database())
        })
    }

    pub fn mark_ready(
        &self,
        setup_id: &str,
        recovery_id: &str,
        backup_id: &str,
        now: &str,
    ) -> AppResult<()> {
        self.with_connection(|connection| {
            let transaction = connection.transaction().map_err(|_| AppError::database())?;
            ensure_pending(&transaction, setup_id)?;

            let recovery_updated = transaction
                .execute(
                    "UPDATE recovery_package_history SET verified_at = ?1
                     WHERE id = ?2 AND installation_id = ?3 AND verified_at IS NULL",
                    params![now, recovery_id, setup_id],
                )
                .map_err(|_| AppError::database())?;
            let backup_updated = transaction
                .execute(
                    "UPDATE backup_history SET verified_at = ?1
                     WHERE id = ?2 AND installation_id = ?3 AND verified_at IS NULL",
                    params![now, backup_id, setup_id],
                )
                .map_err(|_| AppError::database())?;
            if recovery_updated != 1 || backup_updated != 1 {
                return Err(AppError::conflict(
                    "Os artefatos atuais já foram confirmados ou não pertencem a esta instalação.",
                ));
            }

            let installation_updated = transaction
                .execute(
                    "UPDATE installations SET setup_state = 'READY', updated_at = ?1
                     WHERE id = ?2 AND setup_state = 'RECOVERY_PENDING'",
                    params![now, setup_id],
                )
                .map_err(|_| AppError::database())?;
            if installation_updated != 1 {
                return Err(AppError::conflict(
                    "A configuração inicial não está aguardando confirmação.",
                ));
            }

            insert_audit(
                &transaction,
                setup_id,
                "INITIAL_SETUP_COMPLETED",
                "installation",
                Some(setup_id),
                now,
            )?;
            transaction.commit().map_err(|_| AppError::database())
        })
    }

    pub fn create_snapshot(&self, destination: &Path, key: &[u8]) -> AppResult<()> {
        self.with_connection(|source| {
            if destination.exists() {
                return Err(AppError::storage());
            }
            let mut target = open_encrypted_connection(destination, key, true, false)?;
            {
                let backup = Backup::new(source, &mut target).map_err(|_| AppError::database())?;
                backup
                    .run_to_completion(16, Duration::from_millis(25), None)
                    .map_err(|_| AppError::database())?;
            }
            validate_connection(&target)?;
            target
                .execute_batch("PRAGMA journal_mode = DELETE;")
                .map_err(|_| AppError::database())?;
            target.close().map_err(|_| AppError::database())?;
            File::options()
                .read(true)
                .write(true)
                .open(destination)
                .and_then(|file| file.sync_all())
                .map_err(|_| AppError::storage())?;
            verify_encrypted_header(destination)
        })
    }

    pub fn diagnostics(&self) -> AppResult<CipherDiagnostics> {
        self.with_connection(|connection| cipher_diagnostics(connection))
    }

    pub fn find_login_user(&self, username: &str) -> AppResult<Option<LoginUserRecord>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT u.id, u.full_name, u.username, u.email, c.password_phc,
                            GROUP_CONCAT(r.code, ',')
                     FROM users u
                     JOIN installations i ON i.id = u.installation_id
                     JOIN user_credentials c ON c.user_id = u.id
                     JOIN user_roles ur ON ur.user_id = u.id
                     JOIN roles r ON r.id = ur.role_id
                     WHERE u.username = ?1 COLLATE NOCASE
                       AND u.enabled = 1
                       AND i.setup_state = 'READY'
                       AND EXISTS (
                           SELECT 1 FROM user_roles master_ur
                           JOIN roles master_role ON master_role.id = master_ur.role_id
                           WHERE master_ur.user_id = u.id AND master_role.code = 'MASTER_ADMIN'
                       )
                     GROUP BY u.id, u.full_name, u.username, u.email, c.password_phc",
                    [username],
                    |row| {
                        let role_codes: String = row.get(5)?;
                        Ok(LoginUserRecord {
                            user: AuthUser {
                                id: row.get(0)?,
                                full_name: row.get(1)?,
                                username: row.get(2)?,
                                email: row.get(3)?,
                                roles: role_codes.split(',').map(str::to_owned).collect(),
                            },
                            password_phc: Zeroizing::new(row.get(4)?),
                        })
                    },
                )
                .optional()
                .map_err(|_| AppError::database())
        })
    }

    pub fn create_session(&self, session: &NewSessionRecord) -> AppResult<()> {
        self.with_connection(|connection| {
            let transaction = connection.transaction().map_err(|_| AppError::database())?;
            transaction
                .execute(
                    "DELETE FROM sessions
                     WHERE revoked_at IS NOT NULL OR absolute_expires_at <= ?1",
                    [&session.created_at],
                )
                .map_err(|_| AppError::database())?;
            transaction
                .execute(
                    "INSERT INTO sessions
                     (id, user_id, token_hash, csrf_hash, created_at, last_seen_at,
                      last_rotated_at, idle_expires_at, absolute_expires_at, reauthenticated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?5, ?6, ?7, ?5)",
                    params![
                        session.id,
                        session.user_id,
                        session.token_hash.as_slice(),
                        session.csrf_hash.as_slice(),
                        session.created_at,
                        session.idle_expires_at,
                        session.absolute_expires_at,
                    ],
                )
                .map_err(|_| AppError::database())?;
            insert_user_audit_context(
                &transaction,
                &session.user_id,
                "USER_LOGGED_IN",
                "session",
                Some(&session.id),
                &session.created_at,
                "SUCCESS",
                session.correlation_id.as_deref(),
                Some(&session.id),
                &session.source,
            )?;
            transaction.commit().map_err(|_| AppError::database())
        })
    }

    pub fn find_active_session(
        &self,
        token_hash: &[u8; 32],
        now: &str,
    ) -> AppResult<Option<SessionRecord>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT s.id, u.id, u.full_name, u.username, u.email,
                            GROUP_CONCAT(r.code, ','), s.csrf_hash,
                            s.idle_expires_at, s.absolute_expires_at
                     FROM sessions s
                     JOIN users u ON u.id = s.user_id
                     JOIN user_roles ur ON ur.user_id = u.id
                     JOIN roles r ON r.id = ur.role_id
                     WHERE s.token_hash = ?1
                       AND s.revoked_at IS NULL
                       AND s.idle_expires_at > ?2
                       AND s.absolute_expires_at > ?2
                       AND u.enabled = 1
                     GROUP BY s.id, u.id, u.full_name, u.username, u.email,
                              s.csrf_hash, s.idle_expires_at, s.absolute_expires_at",
                    params![token_hash.as_slice(), now],
                    |row| {
                        let role_codes: String = row.get(5)?;
                        let csrf_hash: Vec<u8> = row.get(6)?;
                        let csrf_hash: [u8; 32] = csrf_hash.try_into().map_err(|_| {
                            rusqlite::Error::InvalidColumnType(
                                6,
                                "csrf_hash".to_owned(),
                                rusqlite::types::Type::Blob,
                            )
                        })?;
                        Ok(SessionRecord {
                            id: row.get(0)?,
                            user: AuthUser {
                                id: row.get(1)?,
                                full_name: row.get(2)?,
                                username: row.get(3)?,
                                email: row.get(4)?,
                                roles: role_codes.split(',').map(str::to_owned).collect(),
                            },
                            csrf_hash,
                            idle_expires_at: row.get(7)?,
                            absolute_expires_at: row.get(8)?,
                        })
                    },
                )
                .optional()
                .map_err(|_| AppError::database())
        })
    }

    pub fn touch_session(
        &self,
        session_id: &str,
        token_hash: &[u8; 32],
        now: &str,
        idle_expires_at: &str,
    ) -> AppResult<()> {
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "UPDATE sessions
                     SET last_seen_at = ?1,
                         idle_expires_at = MIN(?2, absolute_expires_at)
                     WHERE id = ?3 AND token_hash = ?4 AND revoked_at IS NULL
                       AND idle_expires_at > ?1 AND absolute_expires_at > ?1",
                    params![now, idle_expires_at, session_id, token_hash.as_slice()],
                )
                .map_err(|_| AppError::database())?;
            if changed != 1 {
                return Err(AppError::unauthenticated());
            }
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rotate_session(
        &self,
        session_id: &str,
        old_token_hash: &[u8; 32],
        new_token_hash: &[u8; 32],
        new_csrf_hash: &[u8; 32],
        now: &str,
        idle_expires_at: &str,
        correlation_id: Option<&str>,
        source: &str,
    ) -> AppResult<()> {
        self.with_connection(|connection| {
            let transaction = connection.transaction().map_err(|_| AppError::database())?;
            let changed = transaction
                .execute(
                    "UPDATE sessions
                     SET token_hash = ?1, csrf_hash = ?2, last_seen_at = ?3,
                         last_rotated_at = ?3,
                         idle_expires_at = MIN(?4, absolute_expires_at)
                     WHERE id = ?5 AND token_hash = ?6 AND revoked_at IS NULL
                       AND idle_expires_at > ?3 AND absolute_expires_at > ?3",
                    params![
                        new_token_hash.as_slice(),
                        new_csrf_hash.as_slice(),
                        now,
                        idle_expires_at,
                        session_id,
                        old_token_hash.as_slice(),
                    ],
                )
                .map_err(|_| AppError::database())?;
            if changed != 1 {
                return Err(AppError::unauthenticated());
            }
            let user_id: String = transaction
                .query_row(
                    "SELECT user_id FROM sessions WHERE id = ?1",
                    [session_id],
                    |row| row.get(0),
                )
                .map_err(|_| AppError::database())?;
            insert_user_audit_context(
                &transaction,
                &user_id,
                "SESSION_SECURITY_ROTATED",
                "session",
                Some(session_id),
                now,
                "SUCCESS",
                correlation_id,
                Some(session_id),
                source,
            )?;
            transaction.commit().map_err(|_| AppError::database())
        })
    }

    pub fn revoke_session(
        &self,
        session_id: &str,
        token_hash: &[u8; 32],
        user_id: &str,
        now: &str,
        correlation_id: Option<&str>,
        source: &str,
    ) -> AppResult<()> {
        self.with_connection(|connection| {
            let transaction = connection.transaction().map_err(|_| AppError::database())?;
            let changed = transaction
                .execute(
                    "UPDATE sessions SET revoked_at = ?1
                     WHERE id = ?2 AND token_hash = ?3 AND user_id = ?4 AND revoked_at IS NULL",
                    params![now, session_id, token_hash.as_slice(), user_id],
                )
                .map_err(|_| AppError::database())?;
            if changed != 1 {
                return Err(AppError::unauthenticated());
            }
            insert_user_audit_context(
                &transaction,
                user_id,
                "USER_LOGGED_OUT",
                "session",
                Some(session_id),
                now,
                "SUCCESS",
                correlation_id,
                Some(session_id),
                source,
            )?;
            transaction.commit().map_err(|_| AppError::database())
        })
    }

    pub fn list_audit_events(
        &self,
        user_id: &str,
        session_id: &str,
        correlation_id: &str,
        limit: u32,
    ) -> AppResult<Vec<AuditEvent>> {
        let limit = i64::from(limit.clamp(1, 100));
        self.with_connection(|connection| {
            let transaction = connection.transaction().map_err(|_| AppError::database())?;
            insert_user_audit_context(
                &transaction,
                user_id,
                "AUDIT_LOG_VIEWED",
                "audit_log",
                None,
                &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "SUCCESS",
                Some(correlation_id),
                Some(session_id),
                "LAN",
            )?;
            let events = {
                let mut statement = transaction
                    .prepare(
                        "SELECT a.id, a.actor_type, a.actor_user_id, u.username,
                                a.action, a.entity_type, a.entity_id, a.result,
                                a.correlation_id, a.session_id, a.source, a.occurred_at
                         FROM audit_events a
                         LEFT JOIN users u ON u.id = a.actor_user_id
                         ORDER BY a.occurred_at DESC, a.id DESC
                         LIMIT ?1",
                    )
                    .map_err(|_| AppError::database())?;
                let rows = statement
                    .query_map([limit], |row| {
                        Ok(AuditEvent {
                            id: row.get(0)?,
                            actor_type: row.get(1)?,
                            actor_user_id: row.get(2)?,
                            actor_username: row.get(3)?,
                            action: row.get(4)?,
                            entity_type: row.get(5)?,
                            entity_id: row.get(6)?,
                            result: row.get(7)?,
                            correlation_id: row.get(8)?,
                            session_id: row.get(9)?,
                            source: row.get(10)?,
                            occurred_at: row.get(11)?,
                        })
                    })
                    .map_err(|_| AppError::database())?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|_| AppError::database())?
            };
            transaction.commit().map_err(|_| AppError::database())?;
            Ok(events)
        })
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut guard = self.connection.lock().map_err(|_| AppError::database())?;
        let connection = guard.as_mut().ok_or_else(AppError::database)?;
        operation(connection)
    }
}

pub fn create_foundation_database(
    path: &Path,
    key: &[u8],
    bootstrap: &BootstrapRecord,
) -> AppResult<CipherDiagnostics> {
    if path.exists() {
        return Err(AppError::storage());
    }
    let mut connection = open_encrypted_connection(path, key, true, false)?;
    connection
        .execute_batch(FOUNDATION_MIGRATION)
        .map_err(|_| AppError::database())?;

    let transaction = connection.transaction().map_err(|_| AppError::database())?;
    insert_bootstrap(&transaction, bootstrap)?;
    transaction.commit().map_err(|_| AppError::database())?;
    migrate_database(&connection)?;
    validate_connection(&connection)?;
    let diagnostics = cipher_diagnostics(&connection)?;
    connection.close().map_err(|_| AppError::database())?;
    File::options()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| AppError::storage())?;
    verify_encrypted_header(path)?;
    Ok(diagnostics)
}

/// Reads the SQLCipher implementation linked into this executable without
/// touching installation paths, DPAPI or network listeners.
pub fn runtime_security_diagnostics() -> AppResult<RuntimeSecurityDiagnostics> {
    let connection = Connection::open_in_memory().map_err(|_| AppError::database())?;
    apply_database_key(&connection, &[0x42; 32])?;
    let diagnostics = cipher_diagnostics(&connection)?;
    let numeric_version = diagnostics
        .version
        .split_whitespace()
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(AppError::security)?;
    Ok(RuntimeSecurityDiagnostics {
        sqlcipher_version: numeric_version.to_owned(),
        minimum_distribution_version: MINIMUM_DISTRIBUTION_SQLCIPHER,
        distribution_ready: diagnostics.distribution_ready,
    })
}

fn migrate_database(connection: &Connection) -> AppResult<()> {
    let mut version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| AppError::database())?;
    if version == 1 {
        connection
            .execute_batch(WEB_IDENTITY_SESSIONS_MIGRATION)
            .map_err(|_| AppError::database())?;
        version = 2;
    }
    if version == 2 {
        connection
            .execute_batch(OPERATIONAL_AUDIT_MIGRATION)
            .map_err(|_| AppError::database())?;
        version = 3;
    }
    if version == CURRENT_SCHEMA_VERSION as i64 {
        Ok(())
    } else {
        Err(AppError::new(
            "UNSUPPORTED_DATABASE_SCHEMA",
            "A versão do banco de dados não é compatível com esta aplicação.",
        ))
    }
}

fn open_encrypted_connection(
    path: &Path,
    key: &[u8],
    create: bool,
    enable_wal: bool,
) -> AppResult<Connection> {
    if key.len() != 32 {
        return Err(AppError::security());
    }
    let reservation = create
        .then(|| reserve_new_database_file(path))
        .transpose()?;
    let result = (|| {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection =
            Connection::open_with_flags(path, flags).map_err(|_| AppError::database())?;

        apply_database_key(&connection, key)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA trusted_schema = OFF;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA secure_delete = ON;
                 PRAGMA busy_timeout = 5000;",
            )
            .map_err(|_| AppError::database())?;
        if enable_wal {
            connection
                .execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;")
                .map_err(|_| AppError::database())?;
        } else {
            connection
                .execute_batch("PRAGMA journal_mode = DELETE; PRAGMA synchronous = FULL;")
                .map_err(|_| AppError::database())?;
        }

        cipher_diagnostics(&connection)?;
        Ok(connection)
    })();
    drop(reservation);
    if create && result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

fn apply_database_key(connection: &Connection, key: &[u8]) -> AppResult<()> {
    if key.len() != 32 {
        return Err(AppError::security());
    }
    // SQLCipher 4.6+ logs ERROR/WARN diagnostics (including page/HMAC details)
    // by default. Disable its process logging before any key or database access;
    // internal causes must never cross the sanitized application boundary.
    connection
        .execute_batch("PRAGMA cipher_log_level = NONE;")
        .map_err(|_| AppError::security())?;
    // The key remains the first operation that can touch database pages; the
    // preceding logging pragma only changes SQLCipher's process diagnostics.
    // The literal is zeroized and never logged or included in a returned error.
    let mut key_pragma = Zeroizing::new(String::with_capacity(83));
    key_pragma.push_str("PRAGMA key = \"x'");
    for byte in key {
        write!(&mut *key_pragma, "{byte:02x}").map_err(|_| AppError::security())?;
    }
    key_pragma.push_str("'\";");
    connection
        .execute_batch(&key_pragma)
        .map_err(|_| AppError::security())
}

fn reserve_new_database_file(path: &Path) -> AppResult<File> {
    let mut options = File::options();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    options.open(path).map_err(|_| AppError::storage())
}

fn cipher_diagnostics(connection: &Connection) -> AppResult<CipherDiagnostics> {
    let cipher_status: String = connection
        .query_row("PRAGMA cipher_status", [], |row| row.get(0))
        .map_err(|_| AppError::security())?;
    if cipher_status != "1" {
        return Err(AppError::security());
    }
    let version: String = connection
        .query_row("PRAGMA cipher_version", [], |row| row.get(0))
        .map_err(|_| AppError::security())?;
    if version.trim().is_empty() {
        return Err(AppError::security());
    }
    let parsed = Version::parse(version.split_whitespace().next().unwrap_or_default())
        .map_err(|_| AppError::security())?;
    let minimum =
        Version::parse(MINIMUM_DISTRIBUTION_SQLCIPHER).expect("valid minimum SQLCipher version");
    Ok(CipherDiagnostics {
        version,
        distribution_ready: parsed >= minimum,
    })
}

fn validate_connection(connection: &Connection) -> AppResult<()> {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| AppError::database())?;
    if integrity != "ok" {
        return Err(AppError::database());
    }

    let cipher_results = pragma_strings(connection, "PRAGMA cipher_integrity_check")?;
    if cipher_results
        .iter()
        .any(|result| !result.is_empty() && result != "ok")
    {
        return Err(AppError::security());
    }

    let foreign_key_violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|_| AppError::database())?;
    if foreign_key_violations != 0 {
        return Err(AppError::database());
    }
    let user_version: u32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| AppError::database())?;
    let installation_versions: (i64, i64) = connection
        .query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN schema_version = ?1 THEN 0 ELSE 1 END)
             FROM installations",
            [CURRENT_SCHEMA_VERSION],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| AppError::database())?;
    if user_version != CURRENT_SCHEMA_VERSION || installation_versions != (1, 0) {
        return Err(AppError::new(
            "UNSUPPORTED_DATABASE_SCHEMA",
            "A versão do banco de dados não é compatível com esta aplicação.",
        ));
    }
    Ok(())
}

fn pragma_strings(connection: &Connection, sql: &str) -> AppResult<Vec<String>> {
    let mut statement = connection.prepare(sql).map_err(|_| AppError::database())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| AppError::database())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::database())
}

fn verify_encrypted_header(path: &Path) -> AppResult<()> {
    let mut file = File::open(path).map_err(|_| AppError::storage())?;
    let mut header = [0_u8; 16];
    file.read_exact(&mut header)
        .map_err(|_| AppError::storage())?;
    if &header == b"SQLite format 3\0" {
        return Err(AppError::security());
    }
    Ok(())
}

fn insert_bootstrap(transaction: &Transaction<'_>, record: &BootstrapRecord) -> AppResult<()> {
    transaction
        .execute(
            "INSERT INTO installations
             (id, database_id, key_id, setup_state, schema_version, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'RECOVERY_PENDING', 1, ?4, ?4)",
            params![
                record.setup_id,
                record.database_id,
                record.key_id,
                record.now,
            ],
        )
        .map_err(|_| AppError::database())?;
    transaction
        .execute(
            "INSERT INTO organizations (id, installation_id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![
                record.organization_id,
                record.setup_id,
                record.organization_name,
                record.now,
            ],
        )
        .map_err(|_| AppError::database())?;
    transaction
        .execute(
            "INSERT INTO units
             (id, organization_id, name, responsible_name, phone, administrative_email,
              address, professional_registration, is_primary, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?9)",
            params![
                record.unit_id,
                record.organization_id,
                record.unit_name,
                record.responsible_name,
                record.phone,
                record.administrative_email,
                record.address,
                record.professional_registration,
                record.now,
            ],
        )
        .map_err(|_| AppError::database())?;
    transaction
        .execute(
            "INSERT INTO users
             (id, installation_id, full_name, username, email, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
            params![
                record.master_user_id,
                record.setup_id,
                record.master_full_name,
                record.master_username,
                record.master_email,
                record.now,
            ],
        )
        .map_err(|_| AppError::database())?;
    transaction
        .execute(
            "INSERT INTO user_credentials (user_id, password_phc, password_changed_at)
             VALUES (?1, ?2, ?3)",
            params![
                record.master_user_id,
                record.password_phc.as_str(),
                record.now
            ],
        )
        .map_err(|_| AppError::database())?;
    transaction
        .execute(
            "INSERT INTO roles (id, installation_id, code, name, created_at)
             VALUES (?1, ?2, 'MASTER_ADMIN', 'Administrador mestre', ?3)",
            params![record.master_role_id, record.setup_id, record.now],
        )
        .map_err(|_| AppError::database())?;
    transaction
        .execute(
            "INSERT INTO user_roles (user_id, role_id, assigned_at) VALUES (?1, ?2, ?3)",
            params![record.master_user_id, record.master_role_id, record.now],
        )
        .map_err(|_| AppError::database())?;
    transaction
        .execute(
            "INSERT INTO installation_settings
             (installation_id, backup_directory, recovery_directory, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                record.setup_id,
                record.backup_directory,
                record.recovery_directory,
                record.now,
            ],
        )
        .map_err(|_| AppError::database())?;
    insert_audit(
        transaction,
        &record.setup_id,
        "INITIAL_SETUP_STARTED",
        "installation",
        Some(&record.setup_id),
        &record.now,
    )
}

fn load_installation(connection: &Connection) -> AppResult<InstallationRecord> {
    let base = connection
        .query_row(
            "SELECT i.id, i.database_id, i.key_id, i.setup_state,
                    s.backup_directory, s.recovery_directory,
                    s.current_recovery_package_id, s.current_backup_id
             FROM installations i
             JOIN installation_settings s ON s.installation_id = i.id
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .map_err(|_| AppError::database())?;

    let recovery_package = match base.6.as_deref() {
        Some(current_id) => connection
            .query_row(
                "SELECT id, file_path, file_name, sha256, created_at
                 FROM recovery_package_history
                 WHERE installation_id = ?1 AND id = ?2",
                params![base.0, current_id],
                |row| {
                    Ok(ArtifactHistoryRecord {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        file_name: row.get(2)?,
                        sha256: row.get(3)?,
                        database_sha256: None,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|_| AppError::database())?,
        None => None,
    };
    let initial_backup = match base.7.as_deref() {
        Some(current_id) => connection
            .query_row(
                "SELECT id, file_path, file_name, sha256, database_sha256, created_at
                 FROM backup_history
                 WHERE installation_id = ?1 AND id = ?2",
                params![base.0, current_id],
                |row| {
                    Ok(ArtifactHistoryRecord {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        file_name: row.get(2)?,
                        sha256: row.get(3)?,
                        database_sha256: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|_| AppError::database())?,
        None => None,
    };

    Ok(InstallationRecord {
        setup_id: base.0,
        database_id: base.1,
        key_id: base.2,
        setup_state: base.3,
        backup_directory: PathBuf::from(base.4),
        recovery_directory: PathBuf::from(base.5),
        recovery_package,
        initial_backup,
    })
}

fn ensure_pending(transaction: &Transaction<'_>, setup_id: &str) -> AppResult<()> {
    let state: Option<String> = transaction
        .query_row(
            "SELECT setup_state FROM installations WHERE id = ?1",
            [setup_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AppError::database())?;
    if state.as_deref() != Some("RECOVERY_PENDING") {
        return Err(AppError::conflict(
            "A configuração inicial não está aguardando recuperação.",
        ));
    }
    Ok(())
}

fn insert_audit(
    transaction: &Transaction<'_>,
    installation_id: &str,
    action: &str,
    entity_type: &str,
    entity_id: Option<&str>,
    now: &str,
) -> AppResult<()> {
    transaction
        .execute(
            "INSERT INTO audit_events
             (id, installation_id, actor_type, actor_user_id, action, entity_type,
              entity_id, metadata_json, occurred_at)
             VALUES (?1, ?2, 'SYSTEM', NULL, ?3, ?4, ?5, '{}', ?6)",
            params![
                uuid::Uuid::now_v7().to_string(),
                installation_id,
                action,
                entity_type,
                entity_id,
                now,
            ],
        )
        .map_err(|_| AppError::database())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_user_audit_context(
    transaction: &Transaction<'_>,
    user_id: &str,
    action: &str,
    entity_type: &str,
    entity_id: Option<&str>,
    now: &str,
    result: &str,
    correlation_id: Option<&str>,
    session_id: Option<&str>,
    source: &str,
) -> AppResult<()> {
    let inserted = transaction
        .execute(
            "INSERT INTO audit_events
             (id, installation_id, actor_type, actor_user_id, action, entity_type,
              entity_id, metadata_json, occurred_at, result, correlation_id, session_id, source)
             SELECT ?1, u.installation_id, 'USER', u.id, ?2, ?3, ?4, '{}', ?5,
                    ?6, ?7, ?8, ?9
             FROM users u WHERE u.id = ?10",
            params![
                uuid::Uuid::now_v7().to_string(),
                action,
                entity_type,
                entity_id,
                now,
                result,
                correlation_id,
                session_id,
                source,
                user_id
            ],
        )
        .map_err(|_| AppError::database())?;
    if inserted != 1 {
        return Err(AppError::database());
    }
    Ok(())
}

pub fn minimum_distribution_sqlcipher() -> &'static str {
    MINIMUM_DISTRIBUTION_SQLCIPHER
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version, password_hash::SaltString};
    use rand_core::{OsRng, RngCore};
    use tempfile::tempdir;

    fn bootstrap() -> BootstrapRecord {
        let mut salt = [0_u8; 16];
        OsRng.fill_bytes(&mut salt);
        let params = Params::new(65_536, 3, 1, Some(32)).expect("argon params");
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let salt = SaltString::encode_b64(&salt).expect("salt");
        let phc = argon
            .hash_password(b"frase longa e exclusiva 2026", &salt)
            .expect("hash")
            .to_string();
        BootstrapRecord {
            setup_id: "01900000-0000-7000-8000-000000000001".to_owned(),
            database_id: "01900000-0000-7000-8000-000000000002".to_owned(),
            key_id: "01900000-0000-7000-8000-000000000003".to_owned(),
            organization_id: "01900000-0000-7000-8000-000000000004".to_owned(),
            unit_id: "01900000-0000-7000-8000-000000000005".to_owned(),
            master_user_id: "01900000-0000-7000-8000-000000000006".to_owned(),
            master_role_id: "01900000-0000-7000-8000-000000000007".to_owned(),
            organization_name: "Clínica Horizonte".to_owned(),
            unit_name: "Unidade principal".to_owned(),
            responsible_name: "Ana Souza".to_owned(),
            phone: None,
            administrative_email: None,
            address: None,
            professional_registration: None,
            master_full_name: "Carlos Oliveira".to_owned(),
            master_username: "carlos.admin".to_owned(),
            master_email: "carlos@example.test".to_owned(),
            password_phc: Zeroizing::new(phc),
            backup_directory: "/tmp/backup".to_owned(),
            recovery_directory: "/tmp/recovery".to_owned(),
            now: "2026-07-22T12:00:00Z".to_owned(),
        }
    }

    #[test]
    fn database_is_encrypted_and_audit_is_append_only() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("database.sqlcipher");
        let key = [7_u8; 32];
        create_foundation_database(&path, &key, &bootstrap()).expect("database");

        let mut header = [0_u8; 16];
        File::open(&path)
            .expect("open")
            .read_exact(&mut header)
            .expect("header");
        assert_ne!(&header, b"SQLite format 3\0");

        let connection = open_encrypted_connection(&path, &key, false, false).expect("open db");
        let update = connection.execute("UPDATE audit_events SET action = 'TAMPERED'", []);
        assert!(update.is_err());
    }

    #[test]
    fn clean_bootstrap_applies_all_forward_migrations() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("database.sqlcipher");
        let key = [7_u8; 32];
        create_foundation_database(&path, &key, &bootstrap()).expect("database");
        let connection = open_encrypted_connection(&path, &key, false, false).expect("open db");
        let user_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user version");
        let schema_version: i64 = connection
            .query_row("SELECT schema_version FROM installations", [], |row| {
                row.get(0)
            })
            .expect("schema version");
        let sessions_table: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'sessions'",
                [],
                |row| row.get(0),
            )
            .expect("sessions table");
        let audit_context_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('audit_events')
                 WHERE name IN ('result', 'correlation_id', 'session_id', 'source')",
                [],
                |row| row.get(0),
            )
            .expect("audit context columns");
        assert_eq!(user_version, CURRENT_SCHEMA_VERSION as i64);
        assert_eq!(schema_version, CURRENT_SCHEMA_VERSION as i64);
        assert_eq!(sessions_table, 1);
        assert_eq!(audit_context_columns, 4);
    }

    #[test]
    fn session_audit_persists_actor_session_source_and_correlation() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("database.sqlcipher");
        let key = [7_u8; 32];
        let bootstrap = bootstrap();
        let user_id = bootstrap.master_user_id.clone();
        create_foundation_database(&path, &key, &bootstrap).expect("database");
        let worker = DatabaseWorker::new(path);
        worker.open(&key).expect("open worker");
        let correlation_id = "019b1234-1234-7123-8123-123456789abc";
        worker
            .create_session(&NewSessionRecord {
                id: "session-audit".to_owned(),
                user_id,
                token_hash: [1_u8; 32],
                csrf_hash: [2_u8; 32],
                created_at: "2026-07-31T12:00:00.000Z".to_owned(),
                idle_expires_at: "2026-07-31T12:30:00.000Z".to_owned(),
                absolute_expires_at: "2026-08-01T00:00:00.000Z".to_owned(),
                correlation_id: Some(correlation_id.to_owned()),
                source: "LAN".to_owned(),
            })
            .expect("create session");

        worker
            .with_connection(|connection| {
                let context: (String, String, String, String) = connection
                    .query_row(
                        "SELECT action, correlation_id, session_id, source
                         FROM audit_events WHERE action = 'USER_LOGGED_IN'",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(|_| AppError::database())?;
                assert_eq!(
                    context,
                    (
                        "USER_LOGGED_IN".to_owned(),
                        correlation_id.to_owned(),
                        "session-audit".to_owned(),
                        "LAN".to_owned(),
                    )
                );
                Ok(())
            })
            .expect("audit context");
    }

    #[test]
    fn wrong_key_fails_closed() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("database.sqlcipher");
        create_foundation_database(&path, &[7_u8; 32], &bootstrap()).expect("database");
        assert!(open_encrypted_connection(&path, &[8_u8; 32], false, false).is_err());
    }

    #[test]
    fn sqlcipher_417_reopens_with_status_and_version() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("database.sqlcipher");
        let key = [7_u8; 32];
        create_foundation_database(&path, &key, &bootstrap()).expect("database");
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .expect("raw open");
        let key_pragma = format!("PRAGMA key = \"x'{}'\";", hex::encode(key));
        connection.execute_batch(&key_pragma).expect("apply key");
        let status: String = connection
            .query_row("PRAGMA cipher_status", [], |row| row.get(0))
            .expect("cipher status");
        let version: String = connection
            .query_row("PRAGMA cipher_version", [], |row| row.get(0))
            .expect("cipher version");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM installations", [], |row| row.get(0))
            .expect("read encrypted database");
        assert_eq!(status, "1");
        assert!(version.starts_with("4.17.0"), "version was {version}");
        assert_eq!(count, 1);
    }

    #[test]
    fn runtime_diagnostics_reads_the_linked_sqlcipher_without_installation_state() {
        let diagnostics = runtime_security_diagnostics().expect("runtime diagnostics");
        assert!(
            semver::Version::parse(&diagnostics.sqlcipher_version)
                .expect("semantic SQLCipher version")
                >= semver::Version::parse(MINIMUM_DISTRIBUTION_SQLCIPHER).expect("minimum version")
        );
        assert!(diagnostics.distribution_ready);
        assert_eq!(
            diagnostics.minimum_distribution_version,
            MINIMUM_DISTRIBUTION_SQLCIPHER
        );
    }

    #[test]
    fn twenty_clients_serialize_session_writes_and_survive_restart() {
        use std::sync::{Arc, Barrier};

        const CLIENTS: usize = 20;
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("database.sqlcipher");
        let key = [7_u8; 32];
        let bootstrap = bootstrap();
        let user_id = bootstrap.master_user_id.clone();
        create_foundation_database(&path, &key, &bootstrap).expect("database");
        let worker = Arc::new(DatabaseWorker::new(path.clone()));
        worker.open(&key).expect("open worker");
        let barrier = Arc::new(Barrier::new(CLIENTS));
        let mut clients = Vec::with_capacity(CLIENTS);

        for index in 0..CLIENTS {
            let worker = worker.clone();
            let barrier = barrier.clone();
            let user_id = user_id.clone();
            clients.push(std::thread::spawn(move || {
                barrier.wait();
                worker
                    .create_session(&NewSessionRecord {
                        id: format!("session-{index:02}"),
                        user_id,
                        token_hash: [index as u8; 32],
                        csrf_hash: [(index + CLIENTS) as u8; 32],
                        created_at: "2026-07-22T12:00:00.000Z".to_owned(),
                        idle_expires_at: "2099-07-22T12:30:00.000Z".to_owned(),
                        absolute_expires_at: "2099-07-23T00:00:00.000Z".to_owned(),
                        correlation_id: None,
                        source: "APPLICATION".to_owned(),
                    })
                    .expect("serialized session write");
            }));
        }
        for client in clients {
            client.join().expect("client thread");
        }

        worker
            .with_connection(|connection| {
                let count: i64 = connection
                    .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                    .map_err(|_| AppError::database())?;
                assert_eq!(count, CLIENTS as i64);
                validate_connection(connection)
            })
            .expect("database integrity");
        drop(worker);

        let reopened = DatabaseWorker::new(path);
        reopened.open(&key).expect("reopen worker");
        let persisted = reopened
            .find_active_session(&[7_u8; 32], "2027-01-01T00:00:00.000Z")
            .expect("read persisted session");
        assert_eq!(persisted.expect("persisted session").id, "session-07");
    }

    #[test]
    fn current_artifact_pair_does_not_depend_on_timestamp_order() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("database.sqlcipher");
        let key = [7_u8; 32];
        let bootstrap = bootstrap();
        let setup_id = bootstrap.setup_id.clone();
        create_foundation_database(&path, &key, &bootstrap).expect("database");
        let worker = DatabaseWorker::new(path);
        worker.open(&key).expect("open worker");

        let first_recovery = artifact("recovery-first", "2026-07-22T12:00:00Z", false);
        let first_backup = artifact("backup-first", "2026-07-22T12:00:00Z", true);
        worker
            .record_artifacts(&setup_id, &first_recovery, &first_backup)
            .expect("first pair");
        let current_recovery = artifact("recovery-current", "2025-01-01T00:00:00Z", false);
        let current_backup = artifact("backup-current", "2025-01-01T00:00:00Z", true);
        worker
            .record_artifacts(&setup_id, &current_recovery, &current_backup)
            .expect("current pair");

        let installation = worker.installation().expect("installation");
        assert_eq!(
            installation.recovery_package.expect("recovery").id,
            "recovery-current"
        );
        assert_eq!(
            installation.initial_backup.expect("backup").id,
            "backup-current"
        );
    }

    #[test]
    fn exclusive_create_does_not_open_or_modify_an_existing_destination() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("existing.sqlcipher");
        let sentinel = b"existing file must remain unchanged";
        std::fs::write(&path, sentinel).expect("write sentinel");

        assert!(open_encrypted_connection(&path, &[7_u8; 32], true, false).is_err());
        assert_eq!(std::fs::read(path).expect("read sentinel"), sentinel);
    }

    fn artifact(id: &str, created_at: &str, backup: bool) -> ArtifactHistoryRecord {
        ArtifactHistoryRecord {
            id: id.to_owned(),
            path: format!("C:/artifacts/{id}"),
            file_name: id.to_owned(),
            sha256: "a".repeat(64),
            database_sha256: backup.then(|| "b".repeat(64)),
            created_at: created_at.to_owned(),
        }
    }
}
