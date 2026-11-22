//! Encryption key for the stored certificate.
//!
//! It lives in the operating system keychain (Keychain, Credential Manager, Secret Service), so the
//! database file alone is not enough to read the certificate. If no keychain is available, it falls
//! back to a file with owner-only permissions in the app data directory.

use rand::RngCore;
use std::{fs, path::Path};

const SERVICE: &str = "app.invoicing.desktop";
const ACCOUNT: &str = "encryption-key";

fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_keychain() -> Result<String, keyring::Error> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
    match entry.get_password() {
        Ok(secret) => Ok(secret),
        Err(keyring::Error::NoEntry) => {
            let secret = random_secret();
            entry.set_password(&secret)?;
            Ok(secret)
        }
        Err(e) => Err(e),
    }
}

fn from_file(dir: &Path) -> Result<String, String> {
    let path = dir.join("secret.key");
    if let Ok(secret) = fs::read_to_string(&path) {
        if secret.trim().len() >= 16 {
            return Ok(secret.trim().to_string());
        }
    }
    let secret = random_secret();
    fs::write(&path, &secret).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(secret)
}

/// Returns the encryption key, creating it on first run.
pub fn load_or_create(data_dir: &Path) -> Result<String, String> {
    match from_keychain() {
        Ok(secret) => Ok(secret),
        Err(e) => {
            eprintln!("keychain unavailable ({e}); using a protected file instead");
            from_file(data_dir)
        }
    }
}
