#[cfg(windows)]
mod windows {
    use windows_sys::Win32::{
        Foundation::{GetLastError, LocalFree},
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };

    fn hex_encode(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            encoded.push(DIGITS[(byte >> 4) as usize] as char);
            encoded.push(DIGITS[(byte & 15) as usize] as char);
        }
        encoded
    }

    fn hex_decode(encoded: &str) -> Result<Vec<u8>, String> {
        if !encoded.is_ascii() || encoded.len() % 2 != 0 {
            return Err("Stored Enterprise credential is invalid.".into());
        }
        (0..encoded.len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&encoded[index..index + 2], 16)
                    .map_err(|_| "Stored Enterprise credential is invalid.".to_string())
            })
            .collect()
    }

    pub fn protect(secret: &str) -> Result<String, String> {
        let bytes = secret.as_bytes();
        let input = CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        let success = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if success == 0 {
            return Err(format!(
                "Could not securely save the Enterprise credential (Windows error {}).",
                unsafe { GetLastError() }
            ));
        }
        let protected =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
        let encoded = format!("dpapi-v1:{}", hex_encode(protected));
        unsafe { LocalFree(output.pbData.cast()) };
        Ok(encoded)
    }

    pub fn unprotect(stored: &str) -> Result<String, String> {
        let encoded = stored
            .strip_prefix("dpapi-v1:")
            .ok_or("Stored Enterprise credential is invalid.")?;
        let bytes = hex_decode(encoded)?;
        let input = CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        let success = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if success == 0 {
            return Err("Could not unlock the Enterprise credential for this Windows user.".into());
        }
        let plaintext =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
        let result = String::from_utf8(plaintext.to_vec())
            .map_err(|_| "Stored Enterprise credential is invalid.".to_string());
        unsafe {
            std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
            LocalFree(output.pbData.cast());
        }
        result
    }
}

#[cfg(windows)]
pub use windows::{protect, unprotect};

#[cfg(not(windows))]
pub fn protect(_secret: &str) -> Result<String, String> {
    Err("Secure Enterprise credential storage is only available on Windows.".into())
}

#[cfg(not(windows))]
pub fn unprotect(_stored: &str) -> Result<String, String> {
    Err("Secure Enterprise credential storage is only available on Windows.".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn user_scoped_dpapi_secret_round_trips_without_plaintext_storage() {
        let encrypted = protect("private-test-key-你好").unwrap();
        assert!(!encrypted.contains("private-test-key"));
        assert_eq!(unprotect(&encrypted).unwrap(), "private-test-key-你好");
    }
}
