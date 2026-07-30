use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{PlatformError, PlatformResult};

const PRODUCT_DIRECTORY: &str = "OfflineDentalSystem";
const ARTIFACT_DIRECTORY: &str = "Artifacts";

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VolumeKind {
    Fixed,
    Removable,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVolume {
    pub id: String,
    pub root_path: String,
    pub label: String,
    pub file_system: String,
    pub available_bytes: u64,
    pub kind: VolumeKind,
    pub writable: bool,
    pub destination_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVolumesResponse {
    pub volumes: Vec<LocalVolume>,
}

#[derive(Clone)]
pub struct ValidatedArtifactLocations {
    pub backup_directory: PathBuf,
    pub recovery_directory: PathBuf,
    pub backup_volume_id: String,
    pub recovery_volume_id: String,
}

#[derive(Clone, Copy)]
pub enum ArtifactKind {
    Backup,
    Recovery,
}

pub struct LocalVolumeProvider;

impl LocalVolumeProvider {
    /// `data_directory` is the product's `%ProgramData%\...\Data` directory.
    /// It is required to derive the controlled sibling artifact path on the
    /// system volume without probing the root of `C:\`.
    pub fn enumerate(data_directory: &Path) -> PlatformResult<Vec<LocalVolume>> {
        validate_data_directory_shape(data_directory)?;
        enumerate_platform_volumes(data_directory)
    }

    /// Resolves opaque volume IDs into controlled directories. Browser-provided
    /// filesystem paths are intentionally not accepted.
    pub fn resolve_artifact_locations(
        backup_volume_id: &str,
        recovery_volume_id: &str,
        data_directory: &Path,
    ) -> PlatformResult<ValidatedArtifactLocations> {
        if !is_valid_volume_id(backup_volume_id)
            || !is_valid_volume_id(recovery_volume_id)
            || backup_volume_id == recovery_volume_id
        {
            return Err(PlatformError::invalid_input());
        }

        let volumes = Self::enumerate(data_directory)?;
        let backup = volumes
            .iter()
            .find(|volume| volume.id == backup_volume_id)
            .ok_or_else(PlatformError::invalid_input)?;
        let recovery = volumes
            .iter()
            .find(|volume| volume.id == recovery_volume_id)
            .ok_or_else(PlatformError::invalid_input)?;

        if !backup.writable || !recovery.writable {
            return Err(PlatformError::storage());
        }

        let backup_directory = controlled_destination(backup, ArtifactKind::Backup)?;
        let recovery_directory = controlled_destination(recovery, ArtifactKind::Recovery)?;
        ensure_separate_from_data(&backup_directory, data_directory)?;
        ensure_separate_from_data(&recovery_directory, data_directory)?;

        Ok(ValidatedArtifactLocations {
            backup_directory,
            recovery_directory,
            backup_volume_id: backup.id.clone(),
            recovery_volume_id: recovery.id.clone(),
        })
    }

    /// Re-enumerates the device and verifies that an already resolved path still
    /// belongs to the expected local volume. Call immediately before writing.
    pub fn revalidate_destination(
        volume_id: &str,
        destination: &Path,
        kind: ArtifactKind,
        data_directory: &Path,
    ) -> PlatformResult<()> {
        if !is_valid_volume_id(volume_id) {
            return Err(PlatformError::invalid_input());
        }
        let volume = Self::enumerate(data_directory)?
            .into_iter()
            .find(|candidate| candidate.id == volume_id)
            .ok_or_else(PlatformError::storage)?;
        if !volume.writable || !paths_equal(destination, &controlled_destination(&volume, kind)?) {
            return Err(PlatformError::security());
        }
        Ok(())
    }

    /// Creates only server-derived artifact directories, resolves junctions and
    /// performs an exclusive write/sync/delete probe. The returned canonical
    /// paths are the only paths that may be persisted by setup.
    pub fn prepare_artifact_locations(
        locations: &ValidatedArtifactLocations,
        data_directory: &Path,
    ) -> PlatformResult<ValidatedArtifactLocations> {
        Self::revalidate_destination(
            &locations.backup_volume_id,
            &locations.backup_directory,
            ArtifactKind::Backup,
            data_directory,
        )?;
        Self::revalidate_destination(
            &locations.recovery_volume_id,
            &locations.recovery_directory,
            ArtifactKind::Recovery,
            data_directory,
        )?;

        let backup = prepare_destination(&locations.backup_directory, data_directory)?;
        let recovery = prepare_destination(&locations.recovery_directory, data_directory)?;
        if roots_equal(&local_drive_root(&backup)?, &local_drive_root(&recovery)?) {
            return Err(PlatformError::invalid_input());
        }

        Ok(ValidatedArtifactLocations {
            backup_directory: backup,
            recovery_directory: recovery,
            backup_volume_id: locations.backup_volume_id.clone(),
            recovery_volume_id: locations.recovery_volume_id.clone(),
        })
    }
}

fn validate_data_directory_shape(data_directory: &Path) -> PlatformResult<()> {
    if !data_directory.is_absolute() || is_unc(data_directory) {
        return Err(PlatformError::invalid_input());
    }
    let name = data_directory
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(PlatformError::invalid_input)?;
    if !name.eq_ignore_ascii_case("Data") || data_directory.parent().is_none() {
        return Err(PlatformError::invalid_input());
    }
    Ok(())
}

fn prepare_destination(destination: &Path, data_directory: &Path) -> PlatformResult<PathBuf> {
    if contains_reparse_in_existing_ancestors(destination)? {
        return Err(PlatformError::security());
    }
    fs::create_dir_all(destination).map_err(|_| PlatformError::storage())?;
    let canonical = fs::canonicalize(destination).map_err(|_| PlatformError::storage())?;
    if is_unc(&canonical) || contains_reparse_component(destination)? {
        return Err(PlatformError::security());
    }
    let expected_root = local_drive_root(destination)?;
    let canonical_root = local_drive_root(&canonical)?;
    if !roots_equal(&expected_root, &canonical_root) {
        return Err(PlatformError::security());
    }
    ensure_separate_from_data(&canonical, data_directory)?;

    let probe = canonical.join(format!(".ods-write-probe-{}", uuid::Uuid::now_v7()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| PlatformError::storage())?;
    file.write_all(b"offline-dental-storage-probe-v1")
        .and_then(|_| file.sync_all())
        .map_err(|_| PlatformError::storage())?;
    drop(file);
    fs::remove_file(&probe).map_err(|_| PlatformError::storage())?;
    Ok(canonical)
}

#[cfg(windows)]
fn contains_reparse_in_existing_ancestors(path: &Path) -> PlatformResult<bool> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if current.parent().is_none() {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Ok(true);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return Err(PlatformError::storage()),
        }
    }
    Ok(false)
}

#[cfg(not(windows))]
fn contains_reparse_in_existing_ancestors(_path: &Path) -> PlatformResult<bool> {
    Err(PlatformError::unavailable())
}

fn ensure_separate_from_data(destination: &Path, data_directory: &Path) -> PlatformResult<()> {
    let destination = normalize_for_comparison(destination);
    let data = normalize_for_comparison(data_directory);
    let destination_prefix = format!("{destination}\\");
    let data_prefix = format!("{data}\\");
    if destination == data
        || destination.starts_with(&data_prefix)
        || data.starts_with(&destination_prefix)
    {
        return Err(PlatformError::invalid_input());
    }
    Ok(())
}

#[cfg(windows)]
fn contains_reparse_component(path: &Path) -> PlatformResult<bool> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if current.parent().is_none() {
            continue;
        }
        let metadata = fs::symlink_metadata(&current).map_err(|_| PlatformError::storage())?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(not(windows))]
fn contains_reparse_component(_path: &Path) -> PlatformResult<bool> {
    Err(PlatformError::unavailable())
}

fn controlled_destination(volume: &LocalVolume, kind: ArtifactKind) -> PlatformResult<PathBuf> {
    let root = Path::new(&volume.root_path);
    let artifact_root = Path::new(&volume.destination_path);
    if !root.is_absolute()
        || is_unc(root)
        || !artifact_root.is_absolute()
        || is_unc(artifact_root)
        || !roots_equal(&local_drive_root(artifact_root)?, root)
    {
        return Err(PlatformError::security());
    }
    let leaf = match kind {
        ArtifactKind::Backup => "Backups",
        ArtifactKind::Recovery => "Recovery",
    };
    Ok(artifact_root.join(leaf))
}

fn opaque_volume_id(root: &str, serial: u32, label: &str, file_system: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"offline-dental-volume-id-v1\0");
    hasher.update(serial.to_le_bytes());
    hasher.update(root.to_ascii_uppercase().as_bytes());
    hasher.update([0]);
    hasher.update(label.as_bytes());
    hasher.update([0]);
    hasher.update(file_system.as_bytes());
    let digest = hasher.finalize();
    format!("volume_{}", URL_SAFE_NO_PAD.encode(&digest[..16]))
}

fn is_valid_volume_id(value: &str) -> bool {
    value.len() == 29
        && value.starts_with("volume_")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(windows)]
fn enumerate_platform_volumes(data_directory: &Path) -> PlatformResult<Vec<LocalVolume>> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};

    use windows_sys::Win32::{
        Storage::FileSystem::{
            GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
        },
        System::{
            SystemServices::FILE_READ_ONLY_VOLUME,
            WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE},
        },
    };

    let required = unsafe { GetLogicalDriveStringsW(0, ptr::null_mut()) };
    if required == 0 || required > 32 * 1024 {
        return Err(PlatformError::unavailable());
    }
    let mut buffer = vec![0_u16; required as usize + 1];
    let written = unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) };
    if written == 0 || written as usize >= buffer.len() {
        return Err(PlatformError::unavailable());
    }

    let mut volumes = Vec::new();
    let mut offset = 0;
    while offset < written as usize {
        let Some(end) = buffer[offset..].iter().position(|value| *value == 0) else {
            return Err(PlatformError::unavailable());
        };
        if end == 0 {
            break;
        }
        let root_os = OsString::from_wide(&buffer[offset..offset + end]);
        let root_path = PathBuf::from(root_os);
        let root = root_path.to_string_lossy().into_owned();
        let root_wide = wide_null(&root_path);
        offset += end + 1;

        let drive_type = unsafe { GetDriveTypeW(root_wide.as_ptr()) };
        let kind = match drive_type {
            DRIVE_FIXED => VolumeKind::Fixed,
            DRIVE_REMOVABLE => VolumeKind::Removable,
            _ => continue,
        };

        let mut label = [0_u16; 261];
        let mut file_system = [0_u16; 32];
        let mut serial = 0_u32;
        let mut maximum_component_length = 0_u32;
        let mut file_system_flags = 0_u32;
        let info_ok = unsafe {
            GetVolumeInformationW(
                root_wide.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                &mut serial,
                &mut maximum_component_length,
                &mut file_system_flags,
                file_system.as_mut_ptr(),
                file_system.len() as u32,
            )
        };
        if info_ok == 0 {
            continue;
        }

        let mut available_bytes = 0_u64;
        let mut total_bytes = 0_u64;
        let mut total_free_bytes = 0_u64;
        let free_space_ok = unsafe {
            GetDiskFreeSpaceExW(
                root_wide.as_ptr(),
                &mut available_bytes,
                &mut total_bytes,
                &mut total_free_bytes,
            )
        };
        if free_space_ok == 0 {
            continue;
        }

        let label = utf16_buffer_to_string(&label);
        let file_system = utf16_buffer_to_string(&file_system);
        let display_label = if label.trim().is_empty() {
            format!("Volume {}", root.trim_end_matches(['\\', '/']))
        } else {
            label
        };
        let destination = artifact_root_for_volume(&root_path, data_directory)?;
        let destination_path = destination.to_string_lossy().into_owned();

        volumes.push(LocalVolume {
            id: opaque_volume_id(&root, serial, &display_label, &file_system),
            root_path: root,
            label: display_label,
            file_system,
            available_bytes,
            kind,
            writable: file_system_flags & FILE_READ_ONLY_VOLUME == 0
                && can_prepare_destination(&destination),
            destination_path,
        });
    }

    volumes.sort_by(|left, right| left.root_path.cmp(&right.root_path));
    Ok(volumes)
}

#[cfg(windows)]
fn can_prepare_destination(destination: &Path) -> bool {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
    };

    let mut existing = destination;
    while !existing.exists() {
        let Some(parent) = existing.parent() else {
            return false;
        };
        existing = parent;
    }
    if !existing.is_dir() {
        return false;
    }
    let wide: Vec<u16> = existing.as_os_str().encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return false;
    }
    unsafe {
        CloseHandle(handle);
    }
    true
}

#[cfg(windows)]
fn wide_null(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn utf16_buffer_to_string(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

#[cfg(not(windows))]
fn enumerate_platform_volumes(_data_directory: &Path) -> PlatformResult<Vec<LocalVolume>> {
    Err(PlatformError::unavailable())
}

fn artifact_root_for_volume(root: &Path, data_directory: &Path) -> PlatformResult<PathBuf> {
    let data_root = local_drive_root(data_directory)?;
    if roots_equal(root, &data_root) {
        let product_root = data_directory
            .parent()
            .ok_or_else(PlatformError::invalid_input)?;
        Ok(product_root.join(ARTIFACT_DIRECTORY))
    } else {
        Ok(root.join(PRODUCT_DIRECTORY).join(ARTIFACT_DIRECTORY))
    }
}

fn local_drive_root(path: &Path) -> PlatformResult<PathBuf> {
    if !path.is_absolute() || is_unc(path) {
        return Err(PlatformError::invalid_input());
    }

    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};

        match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    Ok(PathBuf::from(format!("{}:\\", char::from(letter))))
                }
                _ => Err(PlatformError::invalid_input()),
            },
            _ => Err(PlatformError::invalid_input()),
        }
    }

    #[cfg(not(windows))]
    Err(PlatformError::unavailable())
}

fn is_unc(path: &Path) -> bool {
    let value = path.to_string_lossy().replace('/', "\\");
    (value.starts_with("\\\\") && !value.starts_with("\\\\?\\"))
        || value
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("\\\\?\\UNC\\"))
}

fn roots_equal(left: &Path, right: &Path) -> bool {
    normalize_for_comparison(left) == normalize_for_comparison(right)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    normalize_for_comparison(left) == normalize_for_comparison(right)
}

fn normalize_for_comparison(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::{LocalVolume, VolumeKind, is_valid_volume_id, opaque_volume_id};

    #[cfg(windows)]
    use std::path::Path;

    #[cfg(windows)]
    use super::{artifact_root_for_volume, ensure_separate_from_data};

    #[test]
    fn volume_id_is_opaque_deterministic_and_strict() {
        let id = opaque_volume_id("E:\\", 0x1234_abcd, "BACKUP", "NTFS");
        assert_eq!(id, opaque_volume_id("e:\\", 0x1234_abcd, "BACKUP", "NTFS"));
        assert!(is_valid_volume_id(&id));
        assert!(!id.contains("BACKUP"));
        assert!(!is_valid_volume_id("E:\\"));
    }

    #[test]
    fn dto_uses_the_contract_expected_by_the_web_client() {
        let volume = LocalVolume {
            id: opaque_volume_id("E:\\", 1, "USB", "NTFS"),
            root_path: "E:\\".to_owned(),
            label: "USB".to_owned(),
            file_system: "NTFS".to_owned(),
            available_bytes: 1024,
            kind: VolumeKind::Removable,
            writable: true,
            destination_path: "E:\\OfflineDentalSystem\\Artifacts".to_owned(),
        };
        let value = serde_json::to_value(volume).expect("serialize volume");

        assert_eq!(value["rootPath"], "E:\\");
        assert_eq!(value["availableBytes"], 1024);
        assert_eq!(value["kind"], "removable");
        assert!(value.get("destinationPath").is_some());
    }

    #[cfg(windows)]
    #[test]
    fn artifacts_on_the_data_volume_are_a_sibling_of_data() {
        let data = Path::new(r"C:\ProgramData\OfflineDentalSystem\Data");
        let destination = artifact_root_for_volume(Path::new(r"C:\"), data)
            .expect("controlled destination on system volume");

        assert_eq!(
            destination,
            Path::new(r"C:\ProgramData\OfflineDentalSystem\Artifacts")
        );
        ensure_separate_from_data(&destination, data)
            .expect("artifact path is outside active data");
    }

    #[cfg(windows)]
    #[test]
    fn artifacts_on_an_external_volume_use_the_product_namespace() {
        let data = Path::new(r"C:\ProgramData\OfflineDentalSystem\Data");
        let destination = artifact_root_for_volume(Path::new(r"E:\"), data)
            .expect("controlled destination on external volume");

        assert_eq!(destination, Path::new(r"E:\OfflineDentalSystem\Artifacts"));
    }

    #[cfg(windows)]
    #[test]
    fn active_data_and_its_ancestors_or_descendants_are_rejected() {
        let data = Path::new(r"C:\ProgramData\OfflineDentalSystem\Data");

        assert!(ensure_separate_from_data(data, data).is_err());
        assert!(
            ensure_separate_from_data(
                Path::new(r"C:\ProgramData\OfflineDentalSystem\Data\Backups"),
                data,
            )
            .is_err()
        );
        assert!(
            ensure_separate_from_data(Path::new(r"C:\ProgramData\OfflineDentalSystem"), data)
                .is_err()
        );
    }
}
