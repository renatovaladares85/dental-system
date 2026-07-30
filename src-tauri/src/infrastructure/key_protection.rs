use std::{fs, path::Path};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::domain::{AppError, AppResult};

pub trait KeyProtector: Send + Sync {
    fn protection_name(&self) -> &'static str;
    fn protect(&self, plaintext: &[u8]) -> AppResult<Vec<u8>>;
    fn unprotect(&self, ciphertext: &[u8]) -> AppResult<Zeroizing<Vec<u8>>>;
}

#[derive(Default)]
pub struct PlatformKeyProtector;

impl PlatformKeyProtector {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedKeyFile {
    pub format_version: u16,
    pub database_id: String,
    pub key_id: String,
    pub protection: String,
    pub protected_key_base64: String,
}

impl ProtectedKeyFile {
    pub fn new(database_id: String, key_id: String, protection: &str, blob: &[u8]) -> Self {
        Self {
            format_version: 1,
            database_id,
            key_id,
            protection: protection.to_owned(),
            protected_key_base64: STANDARD.encode(blob),
        }
    }

    pub fn read(path: &Path) -> AppResult<Self> {
        let metadata = fs::metadata(path).map_err(|_| AppError::storage())?;
        if metadata.len() > 64 * 1024 {
            return Err(AppError::security());
        }
        let bytes = fs::read(path).map_err(|_| AppError::storage())?;
        let key_file: Self = serde_json::from_slice(&bytes).map_err(|_| AppError::security())?;
        if key_file.format_version != 1
            || key_file.database_id.is_empty()
            || key_file.key_id.is_empty()
            || key_file.protected_key_base64.len() > 32 * 1024
        {
            return Err(AppError::security());
        }
        Ok(key_file)
    }

    pub fn decode_blob(&self) -> AppResult<Zeroizing<Vec<u8>>> {
        STANDARD
            .decode(&self.protected_key_base64)
            .map(Zeroizing::new)
            .map_err(|_| AppError::security())
    }

    pub fn to_bytes(&self) -> AppResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|_| AppError::storage())
    }
}

#[cfg(windows)]
impl KeyProtector for PlatformKeyProtector {
    fn protection_name(&self) -> &'static str {
        "dpapi-current-user"
    }

    fn protect(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        windows_dpapi::protect(plaintext)
    }

    fn unprotect(&self, ciphertext: &[u8]) -> AppResult<Zeroizing<Vec<u8>>> {
        windows_dpapi::unprotect(ciphertext)
    }
}

#[cfg(not(windows))]
impl KeyProtector for PlatformKeyProtector {
    fn protection_name(&self) -> &'static str {
        "unavailable-non-windows"
    }

    fn protect(&self, _plaintext: &[u8]) -> AppResult<Vec<u8>> {
        Err(AppError::unsupported_platform())
    }

    fn unprotect(&self, _ciphertext: &[u8]) -> AppResult<Zeroizing<Vec<u8>>> {
        Err(AppError::unsupported_platform())
    }
}

#[cfg(windows)]
mod windows_dpapi {
    use std::{ffi::c_void, ptr, slice};

    use windows_sys::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    use zeroize::{Zeroize, Zeroizing};

    use crate::domain::{AppError, AppResult};

    pub fn protect(plaintext: &[u8]) -> AppResult<Vec<u8>> {
        let mut input_copy = Zeroizing::new(plaintext.to_vec());
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(input_copy.len()).map_err(|_| AppError::security())?,
            pbData: input_copy.as_mut_ptr(),
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };

        let succeeded = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if succeeded == 0 || output.pbData.is_null() {
            return Err(AppError::security());
        }

        let protected = unsafe {
            let result = slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            let _ = LocalFree(output.pbData.cast::<c_void>() as HLOCAL);
            result
        };
        Ok(protected)
    }

    pub fn unprotect(ciphertext: &[u8]) -> AppResult<Zeroizing<Vec<u8>>> {
        let mut input_copy = ciphertext.to_vec();
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(input_copy.len()).map_err(|_| AppError::security())?,
            pbData: input_copy.as_mut_ptr(),
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };

        let succeeded = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        input_copy.zeroize();
        if succeeded == 0 || output.pbData.is_null() {
            return Err(AppError::security());
        }

        let plaintext = unsafe {
            let result = Zeroizing::new(
                slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec(),
            );
            ptr::write_bytes(output.pbData, 0, output.cbData as usize);
            let _ = LocalFree(output.pbData.cast::<c_void>() as HLOCAL);
            result
        };
        Ok(plaintext)
    }
}

#[cfg(test)]
pub(crate) struct TestKeyProtector;

#[cfg(test)]
impl KeyProtector for TestKeyProtector {
    fn protection_name(&self) -> &'static str {
        "test-only-plaintext-envelope"
    }

    fn protect(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        let mut result = b"TEST-ONLY\0".to_vec();
        result.extend_from_slice(plaintext);
        Ok(result)
    }

    fn unprotect(&self, ciphertext: &[u8]) -> AppResult<Zeroizing<Vec<u8>>> {
        ciphertext
            .strip_prefix(b"TEST-ONLY\0")
            .map(|value| Zeroizing::new(value.to_vec()))
            .ok_or_else(AppError::security)
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::{KeyProtector, PlatformKeyProtector};

    #[test]
    fn dpapi_round_trip_is_bound_to_the_current_user_context() {
        let protector = PlatformKeyProtector::new();
        let plaintext = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let protected = protector.protect(&plaintext).expect("protect with DPAPI");
        assert_ne!(protected.as_slice(), plaintext.as_slice());
        let recovered = protector
            .unprotect(&protected)
            .expect("unprotect with DPAPI");
        assert_eq!(recovered.len(), 32);
        assert_eq!(recovered.as_slice(), plaintext);
    }
}
