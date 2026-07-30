use std::path::Path;

#[cfg(not(windows))]
use std::fs;

use super::{PlatformError, PlatformResult};

/// Persists a directory entry after an atomic create/rename. Windows requires
/// a directory handle opened for write access before `FlushFileBuffers`.
#[cfg(windows)]
pub(crate) fn sync_directory(directory: &Path) -> PlatformResult<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, FlushFileBuffers, OPEN_EXISTING,
        },
    };

    let wide: Vec<u16> = directory.as_os_str().encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(PlatformError::storage());
    }
    let flushed = unsafe { FlushFileBuffers(handle) };
    unsafe {
        CloseHandle(handle);
    }
    if flushed == 0 {
        return Err(PlatformError::storage());
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn sync_directory(directory: &Path) -> PlatformResult<()> {
    let directory = fs::File::open(directory).map_err(|_| PlatformError::storage())?;
    directory.sync_all().map_err(|_| PlatformError::storage())
}
