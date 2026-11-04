use sha2::{Digest, Sha256};
use windows::{Win32::Security::Cryptography::*, core::PCWSTR};

struct Algorithm(BCRYPT_ALG_HANDLE);
impl Drop for Algorithm {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptCloseAlgorithmProvider(self.0, 0);
        }
    }
}
struct Key(BCRYPT_KEY_HANDLE);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = BCryptDestroyKey(self.0);
        }
    }
}

pub fn verify(message: &[u8], signature: &[u8]) -> Result<(), String> {
    if message.len() > 16_384 || signature.len() != 64 {
        return Err("Invalid update signature or metadata size".into());
    }
    unsafe {
        let mut algorithm = Algorithm(BCRYPT_ALG_HANDLE::default());
        BCryptOpenAlgorithmProvider(
            &mut algorithm.0,
            BCRYPT_ECDSA_P256_ALGORITHM,
            PCWSTR::null(),
            BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
        )
        .ok()
        .map_err(|e| e.to_string())?;
        let mut key = Key(BCRYPT_KEY_HANDLE::default());
        BCryptImportKeyPair(
            algorithm.0,
            None,
            BCRYPT_ECCPUBLIC_BLOB,
            &mut key.0,
            include_bytes!("../../assets/update-public-key.bin"),
            0,
        )
        .ok()
        .map_err(|e| e.to_string())?;
        BCryptVerifySignature(
            key.0,
            None,
            &Sha256::digest(message),
            signature,
            BCRYPT_FLAGS(0),
        )
        .ok()
        .map_err(|_| "Update signature is invalid. Nothing was installed.".into())
    }
}

pub fn nonce() -> Result<String, String> {
    let mut bytes = [0; 16];
    unsafe { BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }
        .ok()
        .map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_metadata_rejects_tampering_and_truncation() {
        let data = include_bytes!("../../assets/update-test.toml");
        let signature = include_bytes!("../../assets/update-test.toml.sig");
        verify(data, signature).unwrap();
        let mut changed = data.to_vec();
        changed[0] ^= 1;
        assert!(verify(&changed, signature).is_err());
        assert!(verify(data, &signature[..63]).is_err());
        let mut wrong = *signature;
        wrong[0] ^= 1;
        assert!(verify(data, &wrong).is_err());
        assert_ne!(nonce().unwrap(), nonce().unwrap());
    }
}
