//! Configuración persistente: datos del emisor, ambiente y certificado (cifrado).

use crate::{
    access_key::Environment,
    crypto::{self, EncryptionKey},
    invoice::Issuer,
    store::{Store, StoreError},
};
use rusqlite::{params, OptionalExtension};

fn environment_text(a: Environment) -> &'static str {
    match a {
        Environment::Testing => "test",
        Environment::Production => "production",
    }
}

pub fn parse_environment(s: &str) -> Option<Environment> {
    match s {
        "test" => Some(Environment::Testing),
        "production" => Some(Environment::Production),
        _ => None,
    }
}

impl Store {
    pub fn save_issuer(&self, owner: i64, e: &Issuer, a: Environment) -> Result<(), StoreError> {
        let c = self.conn.lock().unwrap();
        let json = serde_json::to_string(e).expect("serialize issuer");
        for (k, v) in [
            ("issuer", json.as_str()),
            ("environment", environment_text(a)),
        ] {
            c.execute(
                "INSERT INTO config (owner_id, key, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT (owner_id, key) DO UPDATE SET value = excluded.value",
                params![owner, k, v],
            )?;
        }
        Ok(())
    }

    /// `None` mientras no se haya configurado. El ambiente por defecto es `pruebas`.
    pub fn load_issuer(&self, owner: i64) -> Result<Option<(Issuer, Environment)>, StoreError> {
        let c = self.conn.lock().unwrap();
        let get = |k: &str| -> rusqlite::Result<Option<String>> {
            c.query_row(
                "SELECT value FROM config WHERE owner_id = ?1 AND key = ?2",
                params![owner, k],
                |r| r.get(0),
            )
            .optional()
        };
        let Some(json) = get("issuer")? else {
            return Ok(None);
        };
        let issuer: Issuer = serde_json::from_str(&json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
        let env_label = get("environment")?
            .and_then(|a| parse_environment(&a))
            .unwrap_or(Environment::Testing);
        Ok(Some((issuer, env_label)))
    }

    pub fn save_certificate(
        &self,
        owner: i64,
        key: &EncryptionKey,
        p12: &[u8],
        password: &str,
    ) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO certificate (owner_id, p12_encrypted, password_encrypted) VALUES (?1, ?2, ?3)
             ON CONFLICT (owner_id) DO UPDATE SET p12_encrypted = excluded.p12_encrypted,
                                            password_encrypted = excluded.password_encrypted",
            params![
                owner,
                crypto::encrypt(key, p12),
                crypto::encrypt(key, password.as_bytes())
            ],
        )?;
        Ok(())
    }

    /// `Ok(None)` si no hay certificado; `Err(text)` si no se puede descifrar.
    pub fn load_certificate(
        &self,
        owner: i64,
        key: &EncryptionKey,
    ) -> Result<Option<(Vec<u8>, String)>, String> {
        let row_to_record: Option<(Vec<u8>, Vec<u8>)> = self
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT p12_encrypted, password_encrypted FROM certificate WHERE owner_id = ?1",
                [owner],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some((p12, pass)) = row_to_record else {
            return Ok(None);
        };
        let p12 = crypto::decrypt(key, &p12)?;
        let pass = String::from_utf8(crypto::decrypt(key, &pass)?).map_err(|e| e.to_string())?;
        Ok(Some((p12, pass)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer() -> Issuer {
        Issuer {
            ruc: "1760013210001".into(),
            business_name: "X".into(),
            trade_name: None,
            head_office_address: "Y".into(),
            branch_address: None,
            accounting_required: true,
            establishment: "001".into(),
            emission_point: "002".into(),
        }
    }

    #[test]
    fn issuer_and_environment() {
        let s = Store::in_memory().unwrap();
        assert!(s.load_issuer(1).unwrap().is_none());
        s.save_issuer(1, &issuer(), Environment::Production)
            .unwrap();
        let (e, a) = s.load_issuer(1).unwrap().unwrap();
        assert_eq!(
            (e.emission_point.as_str(), e.accounting_required, a),
            ("002", true, Environment::Production)
        );
        s.save_issuer(1, &issuer(), Environment::Testing).unwrap();
        assert_eq!(s.load_issuer(1).unwrap().unwrap().1, Environment::Testing);
    }

    #[test]
    fn certificate_is_encrypted_in_db() {
        let s = Store::in_memory().unwrap();
        let k = crypto::derive_key("clave-maestra");
        assert!(s.load_certificate(1, &k).unwrap().is_none());
        s.save_certificate(1, &k, b"PKCS12-BYTES", "pass").unwrap();
        let (p12, pass) = s.load_certificate(1, &k).unwrap().unwrap();
        assert_eq!(
            (p12.as_slice(), pass.as_str()),
            (&b"PKCS12-BYTES"[..], "pass")
        );
        // En la base no queda nada legible.
        let raw: Vec<u8> = s
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT p12_encrypted FROM certificate", [], |r| r.get(0))
            .unwrap();
        assert!(!raw.windows(5).any(|w| w == b"PKCS1"));
        assert!(s.load_certificate(1, &crypto::derive_key("otra")).is_err());
        // Reemplazar mantiene un solo registro.
        s.save_certificate(1, &k, b"NUEVO", "p2").unwrap();
        assert_eq!(s.load_certificate(1, &k).unwrap().unwrap().1, "p2");
    }
}
