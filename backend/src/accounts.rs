//! Users, passwords and login sessions.
//!
//! Passwords are stored as Argon2id hashes. A session token is 32 random bytes; only its SHA-256
//! is stored, so a copy of the database cannot be used to impersonate anyone.

use crate::store::{Store, StoreError};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::RngCore;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

/// How long a login stays valid.
pub const SESSION_SECONDS: i64 = 7 * 24 * 3600;
const MIN_PASSWORD_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Everything, including settings, certificate and users.
    Admin,
    /// Issue and query invoices and manage the catalogs.
    Operator,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Operator => "operator",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Role::Admin),
            "operator" => Some(Role::Operator),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub active: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("{0}")]
    Invalid(String),
    #[error("ese usuario ya existe")]
    Duplicate,
    #[error("usuario no encontrado")]
    NotFound,
    #[error("debe quedar al menos un administrador activo")]
    LastAdmin,
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<rusqlite::Error> for AccountError {
    fn from(e: rusqlite::Error) -> Self {
        AccountError::Store(e.into())
    }
}

pub fn validate_username(name: &str) -> Result<String, AccountError> {
    let name = name.trim();
    let ok = (3..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(name.to_string())
    } else {
        Err(AccountError::Invalid(
            "el usuario debe tener entre 3 y 32 caracteres: letras, números, punto, guion o guion bajo".into(),
        ))
    }
}

pub fn validate_password(password: &str) -> Result<(), AccountError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AccountError::Invalid(format!(
            "la contraseña debe tener al menos {MIN_PASSWORD_LEN} caracteres"
        )));
    }
    Ok(())
}

pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashing with default parameters cannot fail")
        .to_string()
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Spends the same time as a real check, so an unknown user cannot be told apart by timing.
pub fn verify_dummy(password: &str) {
    use std::sync::OnceLock;
    static DUMMY: OnceLock<String> = OnceLock::new();
    let hash = DUMMY.get_or_init(|| hash_password("dummy-password"));
    let _ = verify_password(password, hash);
}

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn row_to_user(r: &rusqlite::Row) -> rusqlite::Result<User> {
    let role: String = r.get(2)?;
    Ok(User {
        id: r.get(0)?,
        username: r.get(1)?,
        role: Role::parse(&role).unwrap_or(Role::Operator),
        active: r.get::<_, i64>(3)? != 0,
    })
}

const USER_COLUMNS: &str = "id, username, role, active";

impl Store {
    pub fn count_users(&self) -> Result<i64, AccountError> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)
    }

    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        role: Role,
    ) -> Result<User, AccountError> {
        let username = validate_username(username)?;
        validate_password(password)?;
        let hash = hash_password(password);
        let c = self.conn.lock().unwrap();
        let first: bool = c.query_row("SELECT COUNT(*) = 0 FROM users", [], |r| r.get(0))?;
        match c.execute(
            "INSERT INTO users (username, password_hash, role) VALUES (?1, ?2, ?3)",
            params![username, hash, role.as_str()],
        ) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(AccountError::Duplicate)
            }
            Err(e) => return Err(e.into()),
        }
        let id = c.last_insert_rowid();
        if first {
            // An installation from before accounts: its data now belongs to the first account.
            crate::store::adopt_legacy_data(&c, id)?;
        }
        Ok(User {
            id,
            username,
            role,
            active: true,
        })
    }

    pub fn list_users(&self) -> Result<Vec<User>, AccountError> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare(&format!(
            "SELECT {USER_COLUMNS} FROM users ORDER BY username"
        ))?;
        let users = stmt
            .query_map([], row_to_user)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(users)
    }

    /// The user and the stored hash, for login. Inactive users are returned too; the caller decides.
    pub fn find_login(&self, username: &str) -> Result<Option<(User, String)>, AccountError> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row(
            &format!("SELECT {USER_COLUMNS}, password_hash FROM users WHERE username = ?1"),
            [username.trim()],
            |r| Ok((row_to_user(r)?, r.get::<_, String>(4)?)),
        )
        .optional()?)
    }

    pub fn password_hash_of(&self, id: i64) -> Result<Option<String>, AccountError> {
        let c = self.conn.lock().unwrap();
        Ok(
            c.query_row("SELECT password_hash FROM users WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?,
        )
    }

    /// Changes role, active flag and/or password. Never leaves the system without an active admin.
    pub fn update_user(
        &self,
        id: i64,
        role: Option<Role>,
        active: Option<bool>,
        password: Option<&str>,
    ) -> Result<User, AccountError> {
        if let Some(p) = password {
            validate_password(p)?;
        }
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction()?;
        let current = tx
            .query_row(
                &format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?1"),
                [id],
                row_to_user,
            )
            .optional()?
            .ok_or(AccountError::NotFound)?;
        let new_role = role.unwrap_or(current.role);
        let new_active = active.unwrap_or(current.active);
        let was_active_admin = current.role == Role::Admin && current.active;
        let stays_active_admin = new_role == Role::Admin && new_active;
        if was_active_admin && !stays_active_admin {
            let admins: i64 = tx.query_row(
                "SELECT COUNT(*) FROM users WHERE role = 'admin' AND active = 1",
                [],
                |r| r.get(0),
            )?;
            if admins <= 1 {
                return Err(AccountError::LastAdmin);
            }
        }
        tx.execute(
            "UPDATE users SET role = ?1, active = ?2 WHERE id = ?3",
            params![new_role.as_str(), new_active as i64, id],
        )?;
        if let Some(p) = password {
            tx.execute(
                "UPDATE users SET password_hash = ?1 WHERE id = ?2",
                params![hash_password(p), id],
            )?;
        }
        // A new password, a new role or a deactivation end every open session of that user.
        if password.is_some() || !new_active || new_role != current.role {
            tx.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(User {
            id,
            username: current.username,
            role: new_role,
            active: new_active,
        })
    }

    /// Creates a session and returns its token (shown once, never stored in clear).
    pub fn create_session(&self, user_id: i64) -> Result<(String, i64), AccountError> {
        let token = new_token();
        let expires = now() + SESSION_SECONDS;
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM sessions WHERE expires_at < ?1", [now()])?;
        c.execute(
            "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES (?1, ?2, ?3)",
            params![hash_token(&token), user_id, expires],
        )?;
        Ok((token, expires))
    }

    /// The active user behind a valid, unexpired session token.
    pub fn session_user(&self, token: &str) -> Result<Option<User>, AccountError> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row(
            "SELECT u.id, u.username, u.role, u.active FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ?1 AND s.expires_at > ?2 AND u.active = 1",
            params![hash_token(token), now()],
            row_to_user,
        )
        .optional()?)
    }

    pub fn delete_session(&self, token: &str) -> Result<(), AccountError> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            [hash_token(token)],
        )?;
        Ok(())
    }

    /// Closes the other sessions of a user (after a password change).
    pub fn delete_other_sessions(
        &self,
        user_id: i64,
        keep_token: &str,
    ) -> Result<(), AccountError> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "DELETE FROM sessions WHERE user_id = ?1 AND token_hash <> ?2",
            params![user_id, hash_token(keep_token)],
        )?;
        Ok(())
    }

    pub fn set_password(&self, user_id: i64, password: &str) -> Result<(), AccountError> {
        validate_password(password)?;
        let c = self.conn.lock().unwrap();
        c.execute(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            params![hash_password(password), user_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip_and_rules() {
        let h = hash_password("correct horse");
        assert!(verify_password("correct horse", &h));
        assert!(!verify_password("wrong horse", &h));
        assert!(!verify_password("x", "not-a-hash"));
        assert!(validate_password("short").is_err());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("bad name").is_err());
        assert_eq!(validate_username(" maria.lopez ").unwrap(), "maria.lopez");
    }

    #[test]
    fn users_sessions_and_last_admin() {
        let s = Store::in_memory().unwrap();
        assert_eq!(s.count_users().unwrap(), 0);
        let admin = s
            .create_user("admin", "admin-password", Role::Admin)
            .unwrap();
        assert!(matches!(
            s.create_user("ADMIN", "another-password", Role::Operator),
            Err(AccountError::Duplicate)
        ));
        let op = s
            .create_user("ana", "ana-password", Role::Operator)
            .unwrap();

        let (token, _) = s.create_session(op.id).unwrap();
        assert_eq!(s.session_user(&token).unwrap().unwrap().username, "ana");
        assert!(s.session_user("nope").unwrap().is_none());

        // Deactivating a user ends their sessions.
        s.update_user(op.id, None, Some(false), None).unwrap();
        assert!(s.session_user(&token).unwrap().is_none());

        // The last active admin can be neither demoted nor deactivated.
        assert!(matches!(
            s.update_user(admin.id, Some(Role::Operator), None, None),
            Err(AccountError::LastAdmin)
        ));
        assert!(matches!(
            s.update_user(admin.id, None, Some(false), None),
            Err(AccountError::LastAdmin)
        ));
        // A password change closes the other sessions.
        let (t1, _) = s.create_session(admin.id).unwrap();
        s.update_user(admin.id, None, None, Some("new-admin-password"))
            .unwrap();
        assert!(s.session_user(&t1).unwrap().is_none());
        let (_, hash) = s.find_login("admin").unwrap().unwrap();
        assert!(verify_password("new-admin-password", &hash));
    }

    #[test]
    fn expired_sessions_are_rejected() {
        let s = Store::in_memory().unwrap();
        let u = s.create_user("temp", "temp-password", Role::Admin).unwrap();
        let (token, _) = s.create_session(u.id).unwrap();
        s.conn
            .lock()
            .unwrap()
            .execute("UPDATE sessions SET expires_at = 1", [])
            .unwrap();
        assert!(s.session_user(&token).unwrap().is_none());
    }
}
