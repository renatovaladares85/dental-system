use std::{
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use chrono::Utc;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tempfile::Builder;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{AppError, AppResult, ArtifactReference, SetupArtifacts};

use super::{
    ArtifactHistoryRecord, CURRENT_SCHEMA_VERSION, DatabaseWorker, minimum_distribution_sqlcipher,
};

const RECOVERY_MAGIC: &[u8; 8] = b"ODSKEY\0\x01";
const BACKUP_MAGIC: &[u8; 8] = b"ODSBACK\x01";
const MAX_HEADER_SIZE: usize = 128 * 1024;
const ARGON_MEMORY_KIB: u32 = 65_536;
const ARGON_ITERATIONS: u32 = 3;
const ARGON_PARALLELISM: u32 = 1;

type HmacSha256 = Hmac<Sha256>;

pub struct ArtifactPair {
    pub recovery: ArtifactHistoryRecord,
    pub backup: ArtifactHistoryRecord,
    pub public: SetupArtifacts,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryHeader {
    format: String,
    format_version: u16,
    database_id: String,
    key_id: String,
    created_at: String,
    kdf: KdfDescriptor,
    cipher: CipherDescriptor,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KdfDescriptor {
    algorithm: String,
    version: u32,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    output_bytes: u32,
    salt_base64url: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CipherDescriptor {
    algorithm: String,
    nonce_base64url: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    format: String,
    format_version: u16,
    setup_id: String,
    database_id: String,
    key_id: String,
    backup_id: String,
    created_at: String,
    schema_version: u32,
    sqlcipher_version: String,
    database_file_name: String,
    database_size: u64,
    database_sha256: String,
}

#[allow(clippy::too_many_arguments)]
pub fn create_artifact_pair(
    database: &DatabaseWorker,
    database_key: &[u8],
    setup_id: &str,
    database_id: &str,
    key_id: &str,
    recovery_directory: &Path,
    backup_directory: &Path,
    sqlcipher_version: &str,
) -> AppResult<ArtifactPair> {
    ensure_distinct_directories(recovery_directory, backup_directory)?;

    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let recovery_id = Uuid::now_v7().to_string();
    let backup_id = Uuid::now_v7().to_string();
    let mut recovery_code_bytes = Zeroizing::new([0_u8; 36]);
    OsRng.fill_bytes(recovery_code_bytes.as_mut());
    let recovery_code = Zeroizing::new(URL_SAFE_NO_PAD.encode(recovery_code_bytes.as_ref()));
    let recovery_file_name = format!("offline-dental-recovery-{setup_id}-{recovery_id}.odskey");
    let backup_file_name = format!("offline-dental-backup-{setup_id}-{backup_id}.odsbackup");
    let recovery_path = recovery_directory.join(&recovery_file_name);
    let backup_path = backup_directory.join(&backup_file_name);

    create_recovery_package(
        &recovery_path,
        database_key,
        &recovery_code,
        database_id,
        key_id,
        &now,
    )?;

    let snapshot_path = backup_directory.join(format!(".snapshot-{backup_id}.partial"));
    let backup_result = (|| {
        database.create_snapshot(&snapshot_path, database_key)?;
        let database_sha256 = hash_file(&snapshot_path)?;
        let database_size = fs::metadata(&snapshot_path)
            .map_err(|_| AppError::storage())?
            .len();
        let manifest = BackupManifest {
            format: "OfflineDentalBackup".to_owned(),
            format_version: 1,
            setup_id: setup_id.to_owned(),
            database_id: database_id.to_owned(),
            key_id: key_id.to_owned(),
            backup_id: backup_id.clone(),
            created_at: now.clone(),
            schema_version: CURRENT_SCHEMA_VERSION,
            sqlcipher_version: sqlcipher_version.to_owned(),
            database_file_name: "database.sqlcipher".to_owned(),
            database_size,
            database_sha256: database_sha256.clone(),
        };
        create_backup_container(
            &backup_path,
            &snapshot_path,
            database_key,
            database_id,
            &manifest,
        )?;
        Ok(database_sha256)
    })();
    let _ = fs::remove_file(&snapshot_path);
    let database_sha256 = backup_result?;

    verify_recovery_package(
        &recovery_path,
        &recovery_code,
        database_key,
        database_id,
        key_id,
    )?;
    verify_backup_container(
        &backup_path,
        database_key,
        setup_id,
        database_id,
        key_id,
        &backup_id,
        Some(&database_sha256),
    )?;

    let recovery_sha256 = hash_file(&recovery_path)?;
    let backup_sha256 = hash_file(&backup_path)?;
    let recovery_path = canonical_artifact_path(&recovery_path, recovery_directory)?;
    let backup_path = canonical_artifact_path(&backup_path, backup_directory)?;

    let recovery_reference = ArtifactReference {
        path: recovery_path.to_string_lossy().into_owned(),
        file_name: recovery_file_name.clone(),
        sha256: recovery_sha256.clone(),
    };
    let backup_reference = ArtifactReference {
        path: backup_path.to_string_lossy().into_owned(),
        file_name: backup_file_name.clone(),
        sha256: backup_sha256.clone(),
    };
    let pair = ArtifactPair {
        recovery: ArtifactHistoryRecord {
            id: recovery_id.clone(),
            path: recovery_path.to_string_lossy().into_owned(),
            file_name: recovery_file_name,
            sha256: recovery_sha256,
            database_sha256: None,
            created_at: now.clone(),
        },
        backup: ArtifactHistoryRecord {
            id: backup_id.clone(),
            path: backup_path.to_string_lossy().into_owned(),
            file_name: backup_file_name,
            sha256: backup_sha256,
            database_sha256: Some(database_sha256),
            created_at: now,
        },
        public: SetupArtifacts {
            recovery_package: recovery_reference,
            initial_backup: backup_reference,
            recovery_code: recovery_code.to_string(),
        },
    };
    Ok(pair)
}

#[allow(clippy::too_many_arguments)]
pub fn verify_artifact_pair(
    recovery: &ArtifactHistoryRecord,
    backup: &ArtifactHistoryRecord,
    recovery_directory: &Path,
    backup_directory: &Path,
    recovery_code: &str,
    database_key: &[u8],
    setup_id: &str,
    database_id: &str,
    key_id: &str,
) -> AppResult<()> {
    let recovery_path = canonical_artifact_path(Path::new(&recovery.path), recovery_directory)?;
    let backup_path = canonical_artifact_path(Path::new(&backup.path), backup_directory)?;
    verify_file_identity(&recovery_path, &recovery.file_name, &recovery.sha256)?;
    verify_file_identity(&backup_path, &backup.file_name, &backup.sha256)?;
    verify_recovery_package(
        &recovery_path,
        recovery_code,
        database_key,
        database_id,
        key_id,
    )?;
    verify_backup_container(
        &backup_path,
        database_key,
        setup_id,
        database_id,
        key_id,
        &backup.id,
        backup.database_sha256.as_deref(),
    )
}

fn create_recovery_package(
    path: &Path,
    database_key: &[u8],
    recovery_code: &str,
    database_id: &str,
    key_id: &str,
    created_at: &str,
) -> AppResult<()> {
    if database_key.len() != 32 || recovery_code.len() != 48 {
        return Err(AppError::security());
    }
    let mut salt = [0_u8; 16];
    let mut nonce = [0_u8; 24];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let header = RecoveryHeader {
        format: "OfflineDentalRecoveryKey".to_owned(),
        format_version: 1,
        database_id: database_id.to_owned(),
        key_id: key_id.to_owned(),
        created_at: created_at.to_owned(),
        kdf: KdfDescriptor {
            algorithm: "Argon2id".to_owned(),
            version: 19,
            memory_kib: ARGON_MEMORY_KIB,
            iterations: ARGON_ITERATIONS,
            parallelism: ARGON_PARALLELISM,
            output_bytes: 32,
            salt_base64url: URL_SAFE_NO_PAD.encode(salt),
        },
        cipher: CipherDescriptor {
            algorithm: "XChaCha20-Poly1305".to_owned(),
            nonce_base64url: URL_SAFE_NO_PAD.encode(nonce),
        },
    };
    let header_bytes = serde_json::to_vec(&header).map_err(|_| AppError::security())?;
    if header_bytes.len() > MAX_HEADER_SIZE {
        return Err(AppError::security());
    }
    let mut kek = derive_recovery_key(recovery_code, &salt)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(kek.as_ref()));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: database_key,
                aad: &header_bytes,
            },
        )
        .map_err(|_| AppError::security())?;
    kek.zeroize();

    atomic_write(
        path,
        |file| {
            file.write_all(RECOVERY_MAGIC)?;
            file.write_all(&(header_bytes.len() as u32).to_be_bytes())?;
            file.write_all(&header_bytes)?;
            file.write_all(&ciphertext)
        },
        |temporary_path| {
            verify_recovery_package(
                temporary_path,
                recovery_code,
                database_key,
                database_id,
                key_id,
            )
        },
    )
}

fn verify_recovery_package(
    path: &Path,
    recovery_code: &str,
    expected_key: &[u8],
    expected_database_id: &str,
    expected_key_id: &str,
) -> AppResult<()> {
    let normalized_code = normalize_recovery_code(recovery_code)?;
    let metadata = fs::metadata(path).map_err(|_| AppError::storage())?;
    if metadata.len() > 256 * 1024 {
        return Err(AppError::security());
    }
    let mut file = File::open(path).map_err(|_| AppError::storage())?;
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic)
        .map_err(|_| AppError::security())?;
    if !bool::from(magic.ct_eq(RECOVERY_MAGIC)) {
        return Err(AppError::security());
    }
    let header_length = read_u32(&mut file)? as usize;
    if header_length == 0 || header_length > MAX_HEADER_SIZE {
        return Err(AppError::security());
    }
    let mut header_bytes = vec![0_u8; header_length];
    file.read_exact(&mut header_bytes)
        .map_err(|_| AppError::security())?;
    let header: RecoveryHeader =
        serde_json::from_slice(&header_bytes).map_err(|_| AppError::security())?;
    validate_recovery_header(&header, expected_database_id, expected_key_id)?;
    let salt = decode_fixed::<16>(&header.kdf.salt_base64url)?;
    let nonce = decode_fixed::<24>(&header.cipher.nonce_base64url)?;
    let mut ciphertext = Vec::new();
    file.take(128)
        .read_to_end(&mut ciphertext)
        .map_err(|_| AppError::storage())?;
    if ciphertext.len() != 48 {
        return Err(AppError::security());
    }

    let mut kek = derive_recovery_key(&normalized_code, &salt)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(kek.as_ref()));
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &header_bytes,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| {
            AppError::new(
                "RECOVERY_CODE_INVALID",
                "O código de recuperação é inválido.",
            )
        })?;
    kek.zeroize();
    if plaintext.len() != expected_key.len()
        || !bool::from(plaintext.as_slice().ct_eq(expected_key))
    {
        return Err(AppError::security());
    }
    Ok(())
}

fn validate_recovery_header(
    header: &RecoveryHeader,
    database_id: &str,
    key_id: &str,
) -> AppResult<()> {
    let valid = header.format == "OfflineDentalRecoveryKey"
        && header.format_version == 1
        && header.database_id == database_id
        && header.key_id == key_id
        && header.kdf.algorithm == "Argon2id"
        && header.kdf.version == 19
        && header.kdf.memory_kib == ARGON_MEMORY_KIB
        && header.kdf.iterations == ARGON_ITERATIONS
        && header.kdf.parallelism == ARGON_PARALLELISM
        && header.kdf.output_bytes == 32
        && header.cipher.algorithm == "XChaCha20-Poly1305";
    valid.then_some(()).ok_or_else(AppError::security)
}

fn derive_recovery_key(code: &str, salt: &[u8; 16]) -> AppResult<Zeroizing<[u8; 32]>> {
    let params = Params::new(
        ARGON_MEMORY_KIB,
        ARGON_ITERATIONS,
        ARGON_PARALLELISM,
        Some(32),
    )
    .map_err(|_| AppError::security())?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = Zeroizing::new([0_u8; 32]);
    argon
        .hash_password_into(code.as_bytes(), salt, output.as_mut())
        .map_err(|_| AppError::security())?;
    Ok(output)
}

fn normalize_recovery_code(code: &str) -> AppResult<Zeroizing<String>> {
    if code.len() > 256 {
        return Err(AppError::new(
            "RECOVERY_CODE_INVALID",
            "O código de recuperação é inválido.",
        ));
    }
    let normalized = code
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    if normalized.len() != 48 {
        return Err(AppError::new(
            "RECOVERY_CODE_INVALID",
            "O código de recuperação é inválido.",
        ));
    }
    let decoded = URL_SAFE_NO_PAD.decode(&normalized).map_err(|_| {
        AppError::new(
            "RECOVERY_CODE_INVALID",
            "O código de recuperação é inválido.",
        )
    })?;
    if decoded.len() != 36 {
        return Err(AppError::new(
            "RECOVERY_CODE_INVALID",
            "O código de recuperação é inválido.",
        ));
    }
    Ok(Zeroizing::new(normalized))
}

fn create_backup_container(
    path: &Path,
    snapshot_path: &Path,
    database_key: &[u8],
    database_id: &str,
    manifest: &BackupManifest,
) -> AppResult<()> {
    let manifest_bytes = serde_json::to_vec(manifest).map_err(|_| AppError::storage())?;
    if manifest_bytes.len() > MAX_HEADER_SIZE {
        return Err(AppError::storage());
    }
    let authentication_key = derive_backup_authentication_key(database_key, database_id)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(authentication_key.as_ref())
        .map_err(|_| AppError::security())?;
    mac.update(&manifest_bytes);
    let tag = mac.finalize().into_bytes();

    atomic_write(
        path,
        |output| {
            output.write_all(BACKUP_MAGIC)?;
            output.write_all(&(manifest_bytes.len() as u32).to_be_bytes())?;
            output.write_all(&manifest_bytes)?;
            output.write_all(&tag)?;
            output.write_all(&manifest.database_size.to_be_bytes())?;
            let mut snapshot = File::open(snapshot_path)?;
            std::io::copy(&mut snapshot, output)?;
            Ok(())
        },
        |temporary_path| {
            verify_backup_container(
                temporary_path,
                database_key,
                &manifest.setup_id,
                &manifest.database_id,
                &manifest.key_id,
                &manifest.backup_id,
                Some(&manifest.database_sha256),
            )
        },
    )
}

fn verify_backup_container(
    path: &Path,
    database_key: &[u8],
    expected_setup_id: &str,
    expected_database_id: &str,
    expected_key_id: &str,
    expected_backup_id: &str,
    expected_database_sha256: Option<&str>,
) -> AppResult<()> {
    let mut file = BufReader::new(File::open(path).map_err(|_| AppError::storage())?);
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic)
        .map_err(|_| AppError::security())?;
    if !bool::from(magic.ct_eq(BACKUP_MAGIC)) {
        return Err(AppError::security());
    }
    let manifest_length = read_u32(&mut file)? as usize;
    if manifest_length == 0 || manifest_length > MAX_HEADER_SIZE {
        return Err(AppError::security());
    }
    let mut manifest_bytes = vec![0_u8; manifest_length];
    file.read_exact(&mut manifest_bytes)
        .map_err(|_| AppError::security())?;
    let manifest: BackupManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| AppError::security())?;
    let supported_cipher = manifest
        .sqlcipher_version
        .split_whitespace()
        .next()
        .and_then(|version| semver::Version::parse(version).ok())
        .is_some_and(|version| {
            version
                >= semver::Version::parse(minimum_distribution_sqlcipher())
                    .expect("valid minimum SQLCipher version")
        });
    if manifest.format != "OfflineDentalBackup"
        || manifest.format_version != 1
        || manifest.setup_id != expected_setup_id
        || manifest.database_id != expected_database_id
        || manifest.key_id != expected_key_id
        || manifest.backup_id != expected_backup_id
        || manifest.database_file_name != "database.sqlcipher"
        || manifest.schema_version != CURRENT_SCHEMA_VERSION
        || !supported_cipher
        || chrono::DateTime::parse_from_rfc3339(&manifest.created_at).is_err()
        || expected_database_sha256.is_some_and(|expected| expected != manifest.database_sha256)
    {
        return Err(AppError::security());
    }

    let mut tag = [0_u8; 32];
    file.read_exact(&mut tag)
        .map_err(|_| AppError::security())?;
    let authentication_key = derive_backup_authentication_key(database_key, expected_database_id)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(authentication_key.as_ref())
        .map_err(|_| AppError::security())?;
    mac.update(&manifest_bytes);
    mac.verify_slice(&tag).map_err(|_| AppError::security())?;

    let database_length = read_u64(&mut file)?;
    if database_length != manifest.database_size {
        return Err(AppError::security());
    }
    let mut remaining = database_length;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        let amount =
            usize::try_from(remaining.min(buffer.len() as u64)).map_err(|_| AppError::storage())?;
        file.read_exact(&mut buffer[..amount])
            .map_err(|_| AppError::security())?;
        hasher.update(&buffer[..amount]);
        remaining -= amount as u64;
    }
    let mut trailing = [0_u8; 1];
    if file.read(&mut trailing).map_err(|_| AppError::storage())? != 0 {
        return Err(AppError::security());
    }
    let actual_hash = hex::encode(hasher.finalize());
    if !bool::from(
        actual_hash
            .as_bytes()
            .ct_eq(manifest.database_sha256.as_bytes()),
    ) {
        return Err(AppError::security());
    }
    Ok(())
}

fn derive_backup_authentication_key(
    database_key: &[u8],
    database_id: &str,
) -> AppResult<Zeroizing<[u8; 32]>> {
    if database_key.len() != 32 {
        return Err(AppError::security());
    }
    let hkdf = Hkdf::<Sha256>::new(Some(database_id.as_bytes()), database_key);
    let mut output = Zeroizing::new([0_u8; 32]);
    hkdf.expand(b"ods-backup-manifest-v1", output.as_mut())
        .map_err(|_| AppError::security())?;
    Ok(output)
}

fn ensure_distinct_directories(recovery: &Path, backup: &Path) -> AppResult<()> {
    let recovery = fs::canonicalize(recovery).map_err(|_| AppError::storage())?;
    let backup = fs::canonicalize(backup).map_err(|_| AppError::storage())?;
    if recovery == backup || !recovery.is_dir() || !backup.is_dir() {
        return Err(AppError::validation(vec![crate::domain::FieldError {
            field: "storage".to_owned(),
            message: "Use diretórios existentes e distintos para backup e recuperação.".to_owned(),
        }]));
    }
    Ok(())
}

fn canonical_artifact_path(path: &Path, expected_directory: &Path) -> AppResult<PathBuf> {
    let directory = fs::canonicalize(expected_directory).map_err(|_| AppError::storage())?;
    let artifact = fs::canonicalize(path).map_err(|_| AppError::storage())?;
    if artifact.parent() != Some(directory.as_path()) || !artifact.is_file() {
        return Err(AppError::security());
    }
    Ok(artifact)
}

fn verify_file_identity(path: &Path, expected_name: &str, expected_hash: &str) -> AppResult<()> {
    if path.file_name().and_then(|name| name.to_str()) != Some(expected_name) {
        return Err(AppError::security());
    }
    let actual_hash = hash_file(path)?;
    if !bool::from(actual_hash.as_bytes().ct_eq(expected_hash.as_bytes())) {
        return Err(AppError::security());
    }
    Ok(())
}

fn hash_file(path: &Path) -> AppResult<String> {
    let mut input = BufReader::new(File::open(path).map_err(|_| AppError::storage())?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|_| AppError::storage())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn atomic_write(
    path: &Path,
    operation: impl FnOnce(&mut File) -> std::io::Result<()>,
    validate: impl FnOnce(&Path) -> AppResult<()>,
) -> AppResult<()> {
    let directory = path.parent().ok_or_else(AppError::storage)?;
    if path.exists() {
        return Err(AppError::storage());
    }
    let mut temporary = Builder::new()
        .prefix(".offline-dental-")
        .suffix(".partial")
        .tempfile_in(directory)
        .map_err(|_| AppError::storage())?;
    operation(temporary.as_file_mut()).map_err(|_| AppError::storage())?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(|_| AppError::storage())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| AppError::storage())?;
    validate(temporary.path())?;
    temporary
        .persist_noclobber(path)
        .map_err(|_| AppError::storage())?;
    crate::platform::sync_directory(directory).map_err(|_| AppError::storage())?;
    Ok(())
}

fn decode_fixed<const LENGTH: usize>(value: &str) -> AppResult<[u8; LENGTH]> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::security())?;
    decoded.try_into().map_err(|_| AppError::security())
}

fn read_u32(reader: &mut impl Read) -> AppResult<u32> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| AppError::security())?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> AppResult<u64> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| AppError::security())?;
    Ok(u64::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom};
    use tempfile::tempdir;

    #[test]
    fn recovery_code_has_exactly_forty_eight_characters() {
        let mut bytes = [0_u8; 36];
        OsRng.fill_bytes(&mut bytes);
        let code = URL_SAFE_NO_PAD.encode(bytes);
        assert_eq!(code.len(), 48);
        assert_eq!(URL_SAFE_NO_PAD.decode(code).expect("decode").len(), 36);
    }

    #[test]
    fn validation_failure_does_not_publish_atomic_destination() {
        let directory = tempdir().expect("tempdir");
        let destination = directory.path().join("artifact.odskey");
        let result = atomic_write(
            &destination,
            |file| file.write_all(b"invalid artifact"),
            |_| Err(AppError::security()),
        );

        assert!(result.is_err());
        assert!(!destination.exists());
    }

    #[test]
    fn atomic_publication_never_overwrites_an_existing_destination() {
        let directory = tempdir().expect("tempdir");
        let destination = directory.path().join("artifact.odskey");
        std::fs::write(&destination, b"existing").expect("existing artifact");
        let result = atomic_write(
            &destination,
            |file| file.write_all(b"replacement"),
            |_| Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(destination).expect("read"), b"existing");
    }

    #[test]
    fn recovery_code_preserves_base64url_hyphens() {
        let raw = "ABCDEF-GHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstu";
        assert_eq!(raw.len(), 48);
        let grouped = format!("{} {}", &raw[..24], &raw[24..]);
        assert_eq!(
            normalize_recovery_code(&grouped)
                .expect("normalize")
                .as_str(),
            raw
        );
    }

    #[test]
    fn cryptographic_derivations_match_fixed_vectors() {
        let recovery = derive_recovery_key(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuv",
            &[
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
                0x0e, 0x0f,
            ],
        )
        .expect("derive recovery key");
        assert_eq!(
            hex::encode(recovery.as_ref()),
            "775453a7b59ec60450b18faf53f4f98bb66a3c7577d4623f35426b086d469f0f"
        );

        let database_key: [u8; 32] = std::array::from_fn(|index| index as u8);
        let backup = derive_backup_authentication_key(&database_key, "database-vector-id")
            .expect("derive backup authentication key");
        assert_eq!(
            hex::encode(backup.as_ref()),
            "8b454df4fd0f88f20c349f538a76f45e1583631985111815186c9604bbf8139a"
        );
    }

    #[test]
    fn recovery_package_rejects_a_modified_byte() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("recovery.odskey");
        let key = [42_u8; 32];
        let code = URL_SAFE_NO_PAD.encode([9_u8; 36]);
        create_recovery_package(
            &path,
            &key,
            &code,
            "database-id",
            "key-id",
            "2026-07-22T12:00:00Z",
        )
        .expect("package");
        verify_recovery_package(&path, &code, &key, "database-id", "key-id")
            .expect("valid package");
        let wrong_code = URL_SAFE_NO_PAD.encode([8_u8; 36]);
        assert!(
            verify_recovery_package(&path, &wrong_code, &key, "database-id", "key-id").is_err()
        );

        let mut file = File::options()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open");
        file.seek(SeekFrom::End(-1)).expect("seek");
        let mut last = [0_u8; 1];
        file.read_exact(&mut last).expect("read");
        file.seek(SeekFrom::End(-1)).expect("seek again");
        file.write_all(&[last[0] ^ 0xff]).expect("tamper");
        assert!(verify_recovery_package(&path, &code, &key, "database-id", "key-id").is_err());
    }

    #[test]
    fn backup_rejects_modified_manifest_tag_and_truncation() {
        let directory = tempdir().expect("tempdir");
        let snapshot = directory.path().join("snapshot.sqlcipher");
        let backup = directory.path().join("backup.odsbackup");
        std::fs::write(&snapshot, b"encrypted database fixture").expect("snapshot");
        let database_sha256 = hash_file(&snapshot).expect("snapshot hash");
        let database_key = [17_u8; 32];
        let manifest = BackupManifest {
            format: "OfflineDentalBackup".to_owned(),
            format_version: 1,
            setup_id: "setup-id".to_owned(),
            database_id: "database-id".to_owned(),
            key_id: "key-id".to_owned(),
            backup_id: "backup-id".to_owned(),
            created_at: "2026-07-22T12:00:00Z".to_owned(),
            schema_version: CURRENT_SCHEMA_VERSION,
            sqlcipher_version: "4.17.0 community".to_owned(),
            database_file_name: "database.sqlcipher".to_owned(),
            database_size: std::fs::metadata(&snapshot).expect("metadata").len(),
            database_sha256: database_sha256.clone(),
        };
        create_backup_container(&backup, &snapshot, &database_key, "database-id", &manifest)
            .expect("backup container");
        let verify = || {
            verify_backup_container(
                &backup,
                &database_key,
                "setup-id",
                "database-id",
                "key-id",
                "backup-id",
                Some(&database_sha256),
            )
        };
        verify().expect("valid backup");

        let manifest_offset = 12_u64;
        let mut file = File::options()
            .read(true)
            .write(true)
            .open(&backup)
            .expect("open backup");
        let original_manifest_byte = flip_byte(&mut file, manifest_offset + 5);
        assert!(verify().is_err());
        write_byte(&mut file, manifest_offset + 5, original_manifest_byte);
        verify().expect("restored manifest");

        let manifest_length = serde_json::to_vec(&manifest).expect("manifest bytes").len() as u64;
        let tag_offset = manifest_offset + manifest_length;
        let original_tag_byte = flip_byte(&mut file, tag_offset);
        assert!(verify().is_err());
        write_byte(&mut file, tag_offset, original_tag_byte);
        verify().expect("restored tag");

        let length = file.metadata().expect("backup metadata").len();
        file.set_len(length - 1).expect("truncate backup");
        file.sync_all().expect("sync truncation");
        assert!(verify().is_err());
    }

    fn flip_byte(file: &mut File, offset: u64) -> u8 {
        file.seek(SeekFrom::Start(offset)).expect("seek byte");
        let mut byte = [0_u8; 1];
        file.read_exact(&mut byte).expect("read byte");
        write_byte(file, offset, byte[0] ^ 0xff);
        byte[0]
    }

    fn write_byte(file: &mut File, offset: u64, value: u8) {
        file.seek(SeekFrom::Start(offset)).expect("seek write");
        file.write_all(&[value]).expect("write byte");
        file.flush().expect("flush byte");
        file.sync_all().expect("sync byte");
    }
}
