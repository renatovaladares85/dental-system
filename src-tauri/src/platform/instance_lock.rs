use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

use super::{PlatformError, PlatformResult};

const LOCK_FILE_NAME: &str = ".offline-dental-system.lock";

/// Exclusive process guard for a specific product root.
///
/// The file itself is intentionally persistent. Only its OS lock is released,
/// so a crash never requires deleting or trusting a stale PID file.
pub struct InstanceGuard {
    file: File,
    path: PathBuf,
}

impl InstanceGuard {
    /// Acquire before opening SQLCipher or binding either HTTP listener.
    pub fn acquire(product_root: &Path) -> PlatformResult<Self> {
        if !product_root.is_absolute() {
            return Err(PlatformError::invalid_input());
        }
        let metadata = product_root
            .metadata()
            .map_err(|_| PlatformError::storage())?;
        if !metadata.is_dir() {
            return Err(PlatformError::invalid_input());
        }

        let path = product_root.join(LOCK_FILE_NAME);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|_| PlatformError::storage())?;
        file.try_lock()
            .map_err(|_| PlatformError::new("INSTANCE_ALREADY_RUNNING"))?;

        Ok(Self { file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::InstanceGuard;

    #[test]
    fn second_guard_for_the_same_directory_fails_without_deleting_lock_file() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let first = InstanceGuard::acquire(directory.path()).expect("first lock");
        let lock_path = first.path().to_owned();

        let second = InstanceGuard::acquire(directory.path());
        assert!(second.is_err());
        assert!(lock_path.exists());

        drop(first);
        assert!(lock_path.exists());
        InstanceGuard::acquire(directory.path()).expect("lock after release");
    }
}
