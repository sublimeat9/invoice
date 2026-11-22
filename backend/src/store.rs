//! Persistencia en SQLite: secuenciales y comprobantes emitidos.

use crate::access_key::Environment;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("base de datos: {0}")]
    Db(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub access_key: String,
    pub sequence: u32,
    /// `aaaa-mm-dd`; `None` en comprobantes guardados antes de existir esta columna.
    pub issue_date: Option<String>,
    /// Código de tabla 3 del SRI (`01` factura).
    pub document_type: String,
    pub id_number: Option<String>,
    pub business_name: Option<String>,
    /// Total en centavos.
    pub total_amount: Option<i64>,
    /// `PENDIENTE` (firmado, sin respuesta final), `AUTORIZADO`, `RECHAZADO`, `DEVUELTA`...
    pub status: String,
    pub signed_xml: String,
    pub authorization_number: Option<String>,
    pub authorization_date: Option<String>,
    pub messages: Option<String>,
}

/// Datos del comprobante que se guardan para poder buscarlo después.
#[derive(Debug, Clone)]
pub struct Meta {
    pub issue_date: String,
    pub document_type: String,
    pub id_number: String,
    pub business_name: String,
    pub total_amount: i64,
}

/// Filtros de búsqueda (todos opcionales).
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// `aaaa-mm-dd`, inclusive.
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    /// Prefijo de la identificación del comprador.
    pub id_number: Option<String>,
    pub document_type: Option<String>,
    pub status: Option<String>,
    /// Prefijo de la clave de acceso / número de autorización.
    pub access_key: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub struct Store {
    pub(crate) conn: Mutex<Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sequences (
    owner_id INTEGER NOT NULL DEFAULT 0,
    environment TEXT NOT NULL, establishment TEXT NOT NULL, emission_point TEXT NOT NULL,
    last_value INTEGER NOT NULL,
    PRIMARY KEY (owner_id, environment, establishment, emission_point)
);
CREATE TABLE IF NOT EXISTS config (
    owner_id INTEGER NOT NULL DEFAULT 0,
    key TEXT NOT NULL, value TEXT NOT NULL,
    PRIMARY KEY (owner_id, key)
);
CREATE TABLE IF NOT EXISTS certificate (
    owner_id INTEGER PRIMARY KEY,
    p12_encrypted BLOB NOT NULL,
    password_encrypted BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id INTEGER NOT NULL DEFAULT 0,
    main_code TEXT NOT NULL,
    aux_code TEXT,
    name TEXT NOT NULL,
    price INTEGER NOT NULL,
    vat TEXT NOT NULL DEFAULT '15',
    UNIQUE (owner_id, main_code)
);
CREATE TABLE IF NOT EXISTS customers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id INTEGER NOT NULL DEFAULT 0,
    id_type TEXT NOT NULL,
    id_number TEXT NOT NULL,
    business_name TEXT NOT NULL,
    address TEXT, phone TEXT, email TEXT,
    UNIQUE (owner_id, id_number)
);
CREATE TABLE IF NOT EXISTS documents (
    access_key TEXT PRIMARY KEY,
    owner_id INTEGER NOT NULL DEFAULT 0,
    sequence INTEGER NOT NULL,
    issue_date TEXT,
    document_type TEXT NOT NULL DEFAULT '01',
    id_number TEXT,
    business_name TEXT,
    total_amount INTEGER,
    status TEXT NOT NULL,
    signed_xml TEXT NOT NULL,
    authorization_number TEXT,
    authorization_date TEXT,
    messages TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin', 'operator')),
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);";

fn env_label(a: Environment) -> &'static str {
    match a {
        Environment::Testing => "TEST",
        Environment::Production => "PRODUCTION",
    }
}

impl Store {
    pub fn open(path: &str) -> Result<Self, StoreError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, StoreError> {
        migrate_legacy_spanish_schema(&conn)?;
        migrate_to_owners(&conn)?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Reserva el siguiente secuencial de forma atómica. Un número reservado no se
    /// reutiliza aunque la emisión falle después.
    pub fn reserve_sequence(
        &self,
        owner: i64,
        a: Environment,
        establishment: &str,
        emission_point: &str,
    ) -> Result<u32, StoreError> {
        let c = self.conn.lock().unwrap();
        let n = c.query_row(
            "INSERT INTO sequences (owner_id, environment, establishment, emission_point, last_value) VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT (owner_id, environment, establishment, emission_point) DO UPDATE SET last_value = last_value + 1
             RETURNING last_value",
            params![owner, env_label(a), establishment, emission_point],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// Fija el último secuencial usado (p. ej. al migrar desde otro sistema). Nunca lo
    /// baja: si ya hay uno mayor, se conserva, para no repetir números.
    pub fn set_sequence(
        &self,
        owner: i64,
        a: Environment,
        establishment: &str,
        emission_point: &str,
        last: u32,
    ) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO sequences (owner_id, environment, establishment, emission_point, last_value) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (owner_id, environment, establishment, emission_point) DO UPDATE SET last_value = MAX(last_value, excluded.last_value)",
            params![owner, env_label(a), establishment, emission_point, last],
        )?;
        Ok(())
    }

    pub fn save_pending(
        &self,
        owner: i64,
        access_key: &str,
        sequence: u32,
        signed_xml: &str,
        m: &Meta,
    ) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO documents (owner_id, access_key, sequence, status, signed_xml,
                issue_date, document_type, id_number, business_name, total_amount)
             VALUES (?9, ?1, ?2, 'PENDIENTE', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                access_key,
                sequence,
                signed_xml,
                m.issue_date,
                m.document_type,
                m.id_number,
                m.business_name,
                m.total_amount,
                owner
            ],
        )?;
        Ok(())
    }

    pub fn update_status(
        &self,
        access_key: &str,
        status: &str,
        number: Option<&str>,
        date: Option<&str>,
        messages: Option<&str>,
    ) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "UPDATE documents SET status=?2, authorization_number=?3, authorization_date=?4, messages=?5
             WHERE access_key=?1",
            params![access_key, status, number, date, messages],
        )?;
        Ok(())
    }

    /// Un comprobante de este usuario; los de otros usuarios no existen para él.
    pub fn get_record(&self, owner: i64, access_key: &str) -> Result<Option<Record>, StoreError> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .query_row(
                &format!("{SELECT} WHERE access_key=?1 AND owner_id=?2"),
                params![access_key, owner],
                row_to_record,
            )
            .optional()?)
    }

    /// Busca comprobantes (más recientes primero), sin el XML para aligerar la respuesta.
    /// Devuelve la página pedida y el total de coincidencias.
    pub fn search(&self, owner: i64, f: &Filter) -> Result<(Vec<Record>, u64), StoreError> {
        let mut cond: Vec<&str> = vec!["owner_id = ?"];
        let mut args: Vec<rusqlite::types::Value> = vec![owner.into()];
        let mut add = |c: &'static str, v: String| {
            cond.push(c);
            args.push(v.into());
        };
        // Los `LIKE` usan ESCAPE para que `%` y `_` del usuario sean literales.
        let prefix = |s: &str| {
            format!(
                "{}%",
                s.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        };
        if let Some(v) = &f.date_from {
            add("issue_date >= ?", v.clone());
        }
        if let Some(v) = &f.date_to {
            add("issue_date <= ?", v.clone());
        }
        if let Some(v) = &f.id_number {
            add("id_number LIKE ? ESCAPE '\\'", prefix(v));
        }
        if let Some(v) = &f.document_type {
            add("document_type = ?", v.clone());
        }
        if let Some(v) = &f.status {
            add("status = ?", v.clone());
        }
        if let Some(v) = &f.access_key {
            add(
                "(access_key LIKE ? ESCAPE '\\' OR authorization_number LIKE ? ESCAPE '\\')",
                prefix(v),
            );
            args.push(prefix(v).into());
        }
        let where_clause = format!(" WHERE {}", cond.join(" AND "));

        let c = self.conn.lock().unwrap();
        let total: u64 = c.query_row(
            &format!("SELECT COUNT(*) FROM documents{where_clause}"),
            rusqlite::params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        let limit = f.limit.unwrap_or(50).min(10_000);
        let mut args2 = args.clone();
        args2.push((limit as i64).into());
        args2.push((f.offset.unwrap_or(0) as i64).into());
        let mut st = c.prepare(&format!(
            "SELECT access_key, sequence, issue_date, document_type, id_number, business_name,
                    total_amount, status, '', authorization_number, authorization_date, messages
             FROM documents{where_clause} ORDER BY COALESCE(issue_date, '') DESC, sequence DESC LIMIT ? OFFSET ?"
        ))?;
        let v = st
            .query_map(rusqlite::params_from_iter(args2.iter()), row_to_record)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((v, total))
    }

    /// Pendientes de un usuario, o de todos (`None`, para el reintento en segundo plano).
    pub fn pending(&self, owner: Option<i64>) -> Result<Vec<Record>, StoreError> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare(&format!(
            "{SELECT} WHERE status='PENDIENTE' AND (?1 IS NULL OR owner_id = ?1) ORDER BY owner_id, sequence"
        ))?;
        let v = st
            .query_map([owner], row_to_record)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// Un comprobante por clave de acceso, de cualquier usuario (uso interno del servicio).
    pub(crate) fn get_record_any(&self, access_key: &str) -> Result<Option<Record>, StoreError> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .query_row(
                &format!("{SELECT} WHERE access_key=?1"),
                params![access_key],
                row_to_record,
            )
            .optional()?)
    }
}

const SELECT: &str = "SELECT access_key, sequence, issue_date, document_type, id_number,
    business_name, total_amount, status, signed_xml, authorization_number, authorization_date, messages
    FROM documents";

fn row_to_record(r: &rusqlite::Row) -> rusqlite::Result<Record> {
    Ok(Record {
        access_key: r.get(0)?,
        sequence: r.get(1)?,
        issue_date: r.get(2)?,
        document_type: r.get(3)?,
        id_number: r.get(4)?,
        business_name: r.get(5)?,
        total_amount: r.get(6)?,
        status: r.get(7)?,
        signed_xml: r.get(8)?,
        authorization_number: r.get(9)?,
        authorization_date: r.get(10)?,
        messages: r.get(11)?,
    })
}

/// Renombra tablas, columnas y valores de bases creadas con el esquema en español.
/// No hace nada si la base es nueva o ya está migrada.
fn migrate_legacy_spanish_schema(conn: &Connection) -> Result<(), StoreError> {
    let has_table = |name: &str| -> Result<bool, StoreError> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |r| r.get::<_, i64>(0),
        )? > 0)
    };
    const TABLES: [(&str, &str); 6] = [
        ("secuenciales", "sequences"),
        ("certificado", "certificate"),
        ("productos", "products"),
        ("clientes", "customers"),
        ("comprobantes", "documents"),
        ("config", "config"),
    ];
    const COLUMNS: [(&str, &str, &str); 27] = [
        ("sequences", "ambiente", "environment"),
        ("sequences", "estab", "establishment"),
        ("sequences", "pto_emi", "emission_point"),
        ("sequences", "ultimo", "last_value"),
        ("config", "clave", "key"),
        ("config", "valor", "value"),
        ("certificate", "p12_cifrado", "p12_encrypted"),
        ("certificate", "password_cifrado", "password_encrypted"),
        ("products", "codigo_principal", "main_code"),
        ("products", "codigo_auxiliar", "aux_code"),
        ("products", "nombre", "name"),
        ("products", "precio", "price"),
        ("products", "iva", "vat"),
        ("customers", "tipo_identificacion", "id_type"),
        ("customers", "identificacion", "id_number"),
        ("customers", "razon_social", "business_name"),
        ("customers", "direccion", "address"),
        ("customers", "telefono", "phone"),
        ("documents", "clave_acceso", "access_key"),
        ("documents", "secuencial", "sequence"),
        ("documents", "fecha_emision", "issue_date"),
        ("documents", "tipo_comprobante", "document_type"),
        ("documents", "identificacion", "id_number"),
        ("documents", "razon_social", "business_name"),
        ("documents", "valor_total", "total_amount"),
        ("documents", "estado", "status"),
        ("documents", "xml_firmado", "signed_xml"),
    ];
    if !has_table("comprobantes")? && !has_table("secuenciales")? {
        return Ok(());
    }
    for (old, new) in TABLES {
        if old != new && has_table(old)? {
            conn.execute_batch(&format!("ALTER TABLE {old} RENAME TO {new}"))?;
        }
    }
    for (table, old, new) in COLUMNS {
        if !has_table(table)? {
            continue;
        }
        let cols: Vec<String> = conn
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<_, _>>()?;
        if cols.iter().any(|c| c == old) && !cols.iter().any(|c| c == new) {
            conn.execute_batch(&format!("ALTER TABLE {table} RENAME COLUMN {old} TO {new}"))?;
        }
    }
    if has_table("documents")? {
        for (old, new) in [
            ("numero_autorizacion", "authorization_number"),
            ("fecha_autorizacion", "authorization_date"),
            ("mensajes", "messages"),
            ("creado", "created_at"),
        ] {
            let cols: Vec<String> = conn
                .prepare("PRAGMA table_info(documents)")?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<_, _>>()?;
            if cols.iter().any(|c| c == old) && !cols.iter().any(|c| c == new) {
                conn.execute_batch(&format!(
                    "ALTER TABLE documents RENAME COLUMN {old} TO {new}"
                ))?;
            }
        }
    }
    conn.execute_batch(
        "UPDATE sequences SET environment='TEST' WHERE environment='PRUEBAS';
         UPDATE sequences SET environment='PRODUCTION' WHERE environment='PRODUCCION';
         UPDATE config SET key='issuer' WHERE key='emisor';
         UPDATE config SET key='environment' WHERE key='ambiente';
         UPDATE config SET value='test' WHERE key='environment' AND value='pruebas';
         UPDATE config SET value='production' WHERE key='environment' AND value='produccion';",
    )?;
    if has_table("products")? {
        conn.execute_batch(
            "UPDATE products SET vat='not_subject' WHERE vat='no_objeto';
             UPDATE products SET vat='exempt' WHERE vat='exento';",
        )?;
    }
    Ok(())
}

/// Bases de una sola empresa: los datos pasan a tener dueño. Las filas existentes quedan con
/// `owner_id = 0` y las adopta la primera cuenta que se cree (ver `adopt_legacy_data`).
fn migrate_to_owners(conn: &Connection) -> Result<(), StoreError> {
    let has_owner = |table: &str| -> Result<Option<bool>, StoreError> {
        let cols: Vec<String> = conn
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<_, _>>()?;
        Ok((!cols.is_empty()).then(|| cols.iter().any(|c| c == "owner_id")))
    };
    // (tabla, DDL nuevo, columnas que se copian)
    let rebuilds: [(&str, &str, &str); 5] = [
        (
            "sequences",
            "CREATE TABLE sequences (owner_id INTEGER NOT NULL DEFAULT 0, environment TEXT NOT NULL,
              establishment TEXT NOT NULL, emission_point TEXT NOT NULL, last_value INTEGER NOT NULL,
              PRIMARY KEY (owner_id, environment, establishment, emission_point))",
            "environment, establishment, emission_point, last_value",
        ),
        (
            "config",
            "CREATE TABLE config (owner_id INTEGER NOT NULL DEFAULT 0, key TEXT NOT NULL,
              value TEXT NOT NULL, PRIMARY KEY (owner_id, key))",
            "key, value",
        ),
        (
            "certificate",
            "CREATE TABLE certificate (owner_id INTEGER PRIMARY KEY, p12_encrypted BLOB NOT NULL,
              password_encrypted BLOB NOT NULL)",
            "p12_encrypted, password_encrypted",
        ),
        (
            "products",
            "CREATE TABLE products (id INTEGER PRIMARY KEY AUTOINCREMENT, owner_id INTEGER NOT NULL DEFAULT 0,
              main_code TEXT NOT NULL, aux_code TEXT, name TEXT NOT NULL, price INTEGER NOT NULL,
              vat TEXT NOT NULL DEFAULT '15', UNIQUE (owner_id, main_code))",
            "id, main_code, aux_code, name, price, vat",
        ),
        (
            "customers",
            "CREATE TABLE customers (id INTEGER PRIMARY KEY AUTOINCREMENT, owner_id INTEGER NOT NULL DEFAULT 0,
              id_type TEXT NOT NULL, id_number TEXT NOT NULL, business_name TEXT NOT NULL,
              address TEXT, phone TEXT, email TEXT, UNIQUE (owner_id, id_number))",
            "id, id_type, id_number, business_name, address, phone, email",
        ),
    ];
    for (table, ddl, cols) in rebuilds {
        if has_owner(table)? == Some(false) {
            conn.execute_batch(&format!(
                "ALTER TABLE {table} RENAME TO {table}_old;
                 {ddl};
                 INSERT INTO {table} ({cols}) SELECT {cols} FROM {table}_old;
                 DROP TABLE {table}_old;"
            ))?;
        }
    }
    if has_owner("documents")? == Some(false) {
        conn.execute_batch("ALTER TABLE documents ADD COLUMN owner_id INTEGER NOT NULL DEFAULT 0")?;
    }
    Ok(())
}

/// Asigna a `owner` todo lo que existía antes de haber cuentas (instalaciones de una sola
/// empresa). Se llama al crear el primer usuario.
pub(crate) fn adopt_legacy_data(c: &Connection, owner: i64) -> Result<(), StoreError> {
    for table in [
        "sequences",
        "config",
        "certificate",
        "products",
        "customers",
        "documents",
    ] {
        c.execute(
            &format!("UPDATE {table} SET owner_id = ?1 WHERE owner_id = 0"),
            [owner],
        )?;
    }
    Ok(())
}

/// Agrega las columnas nuevas a bases creadas por versiones anteriores.
fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let existing: Vec<String> = conn
        .prepare("PRAGMA table_info(documents)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    for (col, def) in [
        ("issue_date", "TEXT"),
        ("document_type", "TEXT NOT NULL DEFAULT '01'"),
        ("id_number", "TEXT"),
        ("business_name", "TEXT"),
        ("total_amount", "INTEGER"),
    ] {
        if !existing.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE documents ADD COLUMN {col} {def}"), [])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_increment_per_series_and_environment() {
        let s = Store::in_memory().unwrap();
        let p = Environment::Testing;
        assert_eq!(s.reserve_sequence(1, p, "001", "001").unwrap(), 1);
        assert_eq!(s.reserve_sequence(1, p, "001", "001").unwrap(), 2);
        assert_eq!(s.reserve_sequence(1, p, "001", "002").unwrap(), 1);
        assert_eq!(
            s.reserve_sequence(1, Environment::Production, "001", "001")
                .unwrap(),
            1
        );
        s.set_sequence(1, p, "001", "001", 100).unwrap();
        assert_eq!(s.reserve_sequence(1, p, "001", "001").unwrap(), 101);
        s.set_sequence(1, p, "001", "001", 5).unwrap(); // no debe bajar
        assert_eq!(s.reserve_sequence(1, p, "001", "001").unwrap(), 102);
    }

    fn meta(date: &str, id_number: &str, total: i64) -> Meta {
        Meta {
            issue_date: date.into(),
            document_type: "01".into(),
            id_number: id_number.into(),
            business_name: "CLIENTE".into(),
            total_amount: total,
        }
    }

    #[test]
    fn document_lifecycle() {
        let s = Store::in_memory().unwrap();
        s.save_pending(1, "K1", 1, "<x/>", &meta("2026-11-20", "1712345678", 1725))
            .unwrap();
        assert_eq!(s.pending(None).unwrap().len(), 1);
        s.update_status("K1", "AUTORIZADO", Some("K1"), Some("2026-11-22"), None)
            .unwrap();
        let r = s.get_record(1, "K1").unwrap().unwrap();
        assert_eq!(r.status, "AUTORIZADO");
        assert_eq!(r.total_amount, Some(1725));
        assert_eq!(r.authorization_number.as_deref(), Some("K1"));
        assert!(s.pending(None).unwrap().is_empty());
        assert!(s.get_record(1, "nada").unwrap().is_none());
        assert!(s
            .save_pending(1, "K1", 2, "<y/>", &meta("2026-11-20", "1", 1))
            .is_err());
    }

    #[test]
    fn search_with_filters_and_paging() {
        let s = Store::in_memory().unwrap();
        s.save_pending(1, "A1", 1, "<x/>", &meta("2026-11-20", "1712345678", 100))
            .unwrap();
        s.save_pending(
            1,
            "A2",
            2,
            "<x/>",
            &meta("2026-11-21", "9999999999999", 200),
        )
        .unwrap();
        s.save_pending(1, "B3", 3, "<x/>", &meta("2026-11-22", "1712345678", 300))
            .unwrap();
        s.update_status("B3", "AUTORIZADO", Some("NUM9"), None, None)
            .unwrap();

        let (all, n) = s.search(1, &Filter::default()).unwrap();
        assert_eq!(n, 3);
        assert_eq!(all[0].access_key, "B3"); // más reciente primero
        assert!(all.iter().all(|r| r.signed_xml.is_empty()));

        let f = |f: Filter| s.search(1, &f).unwrap();
        let (v, n) = f(Filter {
            date_from: Some("2026-11-21".into()),
            date_to: Some("2026-11-21".into()),
            ..Default::default()
        });
        assert_eq!((n, v[0].access_key.as_str()), (1, "A2"));
        assert_eq!(
            f(Filter {
                id_number: Some("17123".into()),
                ..Default::default()
            })
            .1,
            2
        );
        assert_eq!(
            f(Filter {
                status: Some("AUTORIZADO".into()),
                ..Default::default()
            })
            .1,
            1
        );
        assert_eq!(
            f(Filter {
                access_key: Some("NUM".into()),
                ..Default::default()
            })
            .1,
            1
        ); // por número de autorización
        assert_eq!(
            f(Filter {
                access_key: Some("A".into()),
                ..Default::default()
            })
            .1,
            2
        );
        // `%` y `_` se tratan literalmente.
        assert_eq!(
            f(Filter {
                id_number: Some("%".into()),
                ..Default::default()
            })
            .1,
            0
        );
        let (page, total) = f(Filter {
            limit: Some(2),
            offset: Some(2),
            ..Default::default()
        });
        assert_eq!((page.len(), total), (1, 3));
    }

    #[test]
    fn migrates_old_database() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE documents (access_key TEXT PRIMARY KEY, sequence INTEGER NOT NULL,
             status TEXT NOT NULL, signed_xml TEXT NOT NULL, authorization_number TEXT,
             authorization_date TEXT, messages TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')));
             INSERT INTO documents (access_key, sequence, status, signed_xml) VALUES ('OLD', 1, 'AUTORIZADO', '<x/>');",
        )
        .unwrap();
        let s = Store::from_connection(c).unwrap();
        let r = s.get_record(0, "OLD").unwrap().unwrap();
        assert_eq!(
            (r.issue_date, r.total_amount, r.document_type.as_str()),
            (None, None, "01")
        );
        assert_eq!(s.search(0, &Filter::default()).unwrap().1, 1);
    }

    #[test]
    fn first_account_adopts_pre_account_data() {
        let s = Store::in_memory().unwrap();
        // Data written before there were accounts belongs to owner 0.
        s.reserve_sequence(0, Environment::Testing, "001", "001")
            .unwrap();
        s.save_pending(0, "OLD", 1, "<x/>", &meta("2026-11-20", "1", 1))
            .unwrap();
        let first = s
            .create_user("owner", "owner-password", crate::accounts::Role::Admin)
            .unwrap();
        let second = s
            .create_user("other", "other-password", crate::accounts::Role::Operator)
            .unwrap();
        assert!(s.get_record(first.id, "OLD").unwrap().is_some());
        assert!(s.get_record(second.id, "OLD").unwrap().is_none());
        assert_eq!(
            s.reserve_sequence(first.id, Environment::Testing, "001", "001")
                .unwrap(),
            2
        );
        assert_eq!(
            s.reserve_sequence(second.id, Environment::Testing, "001", "001")
                .unwrap(),
            1
        );
    }

    #[test]
    fn migrates_spanish_schema_in_place() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE secuenciales (ambiente TEXT NOT NULL, estab TEXT NOT NULL, pto_emi TEXT NOT NULL,
               ultimo INTEGER NOT NULL, PRIMARY KEY (ambiente, estab, pto_emi));
             INSERT INTO secuenciales VALUES ('PRUEBAS','001','001',7);
             CREATE TABLE config (clave TEXT PRIMARY KEY, valor TEXT NOT NULL);
             INSERT INTO config VALUES ('ambiente','produccion');
             CREATE TABLE comprobantes (clave_acceso TEXT PRIMARY KEY, secuencial INTEGER NOT NULL,
               estado TEXT NOT NULL, xml_firmado TEXT NOT NULL, numero_autorizacion TEXT,
               fecha_autorizacion TEXT, mensajes TEXT, creado TEXT NOT NULL DEFAULT (datetime('now')));
             INSERT INTO comprobantes (clave_acceso, secuencial, estado, xml_firmado) VALUES ('K', 7, 'AUTORIZADO', '<x/>');",
        )
        .unwrap();
        let s = Store::from_connection(c).unwrap();
        assert_eq!(s.get_record(0, "K").unwrap().unwrap().sequence, 7);
        assert_eq!(
            s.reserve_sequence(0, Environment::Testing, "001", "001")
                .unwrap(),
            8
        );
        let env: String = s
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT value FROM config WHERE key='environment'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(env, "production");
    }
}
