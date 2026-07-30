use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{PlatformError, PlatformResult, sync_directory};

const HOST_IDENTITY_FILE: &str = "host-identity.json";
const MAX_IDENTITY_BYTES: u64 = 8 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostIdentity {
    pub format_version: u16,
    pub installation_id: String,
    pub hostname: String,
    pub created_at: DateTime<Utc>,
}

impl HostIdentity {
    fn generate() -> Self {
        let installation_id = Uuid::now_v7().to_string();
        Self {
            format_version: 1,
            hostname: format!("dental-{installation_id}.local"),
            installation_id,
            created_at: Utc::now(),
        }
    }

    fn validate(&self) -> PlatformResult<()> {
        let parsed =
            Uuid::parse_str(&self.installation_id).map_err(|_| PlatformError::security())?;
        let expected_hostname = format!("dental-{parsed}.local");
        if self.format_version != 1 || self.hostname != expected_hostname {
            return Err(PlatformError::security());
        }
        Ok(())
    }
}

pub struct HostIdentityManager;

impl HostIdentityManager {
    /// Must be called while holding `InstanceGuard`. The resulting ID is also
    /// the installation ID that setup persists, avoiding a second identity.
    pub fn load_or_create(product_root: &Path) -> PlatformResult<HostIdentity> {
        if !product_root.is_absolute()
            || !product_root
                .metadata()
                .map(|metadata| metadata.is_dir())
                .unwrap_or(false)
        {
            return Err(PlatformError::invalid_input());
        }
        let path = product_root.join(HOST_IDENTITY_FILE);
        match Self::read(&path) {
            Ok(identity) => return Ok(identity),
            Err(error) if error.code() != "HOST_IDENTITY_NOT_FOUND" => return Err(error),
            Err(_) => {}
        }

        let identity = HostIdentity::generate();
        identity.validate()?;
        let bytes = serde_json::to_vec(&identity).map_err(|_| PlatformError::storage())?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(product_root).map_err(|_| PlatformError::storage())?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| PlatformError::storage())?;

        match temporary.persist_noclobber(&path) {
            Ok(file) => {
                file.sync_all().map_err(|_| PlatformError::storage())?;
                sync_directory(product_root)?;
                Ok(identity)
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                Self::read(&path)
            }
            Err(_) => Err(PlatformError::storage()),
        }
    }

    pub fn read(path: &Path) -> PlatformResult<HostIdentity> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(PlatformError::new("HOST_IDENTITY_NOT_FOUND"));
            }
            Err(_) => return Err(PlatformError::storage()),
        };
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_IDENTITY_BYTES {
            return Err(PlatformError::security());
        }
        let file = File::open(path).map_err(|_| PlatformError::storage())?;
        let identity: HostIdentity =
            serde_json::from_reader(file).map_err(|_| PlatformError::security())?;
        identity.validate()?;
        Ok(identity)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::HostIdentityManager;

    #[test]
    fn identity_is_created_once_and_reused() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let first = HostIdentityManager::load_or_create(directory.path()).expect("create identity");
        let second = HostIdentityManager::load_or_create(directory.path()).expect("load identity");

        assert_eq!(first.installation_id, second.installation_id);
        assert_eq!(first.hostname, second.hostname);
    }

    #[test]
    fn corrupt_identity_fails_closed_instead_of_being_replaced() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("host-identity.json");
        fs::write(path, b"{\"formatVersion\":1}").expect("write corrupt identity");

        assert!(HostIdentityManager::load_or_create(directory.path()).is_err());
    }
}
