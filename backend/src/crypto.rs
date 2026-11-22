//! Cifrado en reposo (AES-256-GCM) del certificado y su contraseña.
//! La clave sale de `ENCRYPTION_KEY` (variable de entorno), nunca de la base de datos.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub type EncryptionKey = [u8; 32];

pub fn derive_key(secret: &str) -> EncryptionKey {
    Sha256::digest(secret.as_bytes()).into()
}

/// Devuelve `nonce (12 bytes) || text cifrado`.
pub fn encrypt(key: &EncryptionKey, data: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let mut out = nonce.to_vec();
    out.extend(
        cipher
            .encrypt(Nonce::from_slice(&nonce), data)
            .expect("cifrado AES-GCM"),
    );
    out
}

pub fn decrypt(key: &EncryptionKey, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 12 + 16 {
        return Err("datos cifrados truncados".into());
    }
    let (nonce, ct) = data.split_at(12);
    Aes256Gcm::new(key.into())
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| "no se pudo descifrar (¿cambió ENCRYPTION_KEY?)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let k = derive_key("una-clave-larga-de-prueba");
        let c = encrypt(&k, b"secreto");
        assert_ne!(&c[12..], b"secreto");
        assert_eq!(decrypt(&k, &c).unwrap(), b"secreto");
        // Nonce distinto en cada cifrado.
        assert_ne!(encrypt(&k, b"secreto"), c);
    }

    #[test]
    fn wrong_key_or_tampered_fails() {
        let k = derive_key("a");
        let mut c = encrypt(&k, b"secreto");
        assert!(decrypt(&derive_key("b"), &c).is_err());
        *c.last_mut().unwrap() ^= 1;
        assert!(decrypt(&k, &c).is_err());
        assert!(decrypt(&k, b"corto").is_err());
    }
}
