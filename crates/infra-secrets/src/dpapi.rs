//! Windows DPAPI wrapper.
//!
//! Stage 0: только функция `protect` / `unprotect` поверх `CryptProtectData` /
//! `CryptUnprotectData`. Полноценный vault (файл `vault.key` + AES-GCM поверх
//! записей) реализуется на Stage 5.

#![cfg(windows)]

use std::io;

use windows::Win32::Foundation::LocalFree;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

/// Шифрует `plain` через DPAPI (user-scope).
pub fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
    let mut in_buf = plain.to_vec();
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: in_buf.len() as u32,
        pbData: in_buf.as_mut_ptr(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(&in_blob, None, None, None, None, 0, &mut out_blob)
            .map_err(|e| io::Error::other(format!("CryptProtectData: {e}")))?;
    }

    let result =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();

    unsafe {
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(out_blob.pbData as _));
    }

    Ok(result)
}

/// Дешифрует `enc` через DPAPI (user-scope).
pub fn unprotect(enc: &[u8]) -> io::Result<Vec<u8>> {
    let mut in_buf = enc.to_vec();
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: in_buf.len() as u32,
        pbData: in_buf.as_mut_ptr(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(&in_blob, None, None, None, None, 0, &mut out_blob)
            .map_err(|e| io::Error::other(format!("CryptUnprotectData: {e}")))?;
    }

    let result =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();

    unsafe {
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(out_blob.pbData as _));
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protect_roundtrip() {
        let original = b"super-secret-payload";
        let enc = protect(original).expect("protect");
        assert_ne!(enc, original);
        let dec = unprotect(&enc).expect("unprotect");
        assert_eq!(dec, original);
    }
}
