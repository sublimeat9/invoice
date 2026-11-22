//! Almacenamiento en Postgres (p. ej. Supabase). Misma interfaz que SQLite (`db::Db`); pensado
//! para entornos sin disco persistente como Vercel.
//!
//! Con el pooler de Supabase en modo transacción (puerto 6543) no se pueden usar sentencias
//! preparadas, por eso se desactiva su caché.

use crate::{
    access_key::Environment,
    accounts::{
        hash_password, hash_token, new_token, now, validate_password, validate_username,
        AccountError, Role, User, SESSION_SECONDS,
    },
    catalog::{CatalogError, Customer, Product},
    crypto::{self, EncryptionKey},
    db::Db,
    invoice::Issuer,
    settings::{environment_text, parse_environment},
    store::{env_label, Filter, Meta, Record, StoreError},
};
use async_trait::async_trait;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions, PgRow},
    PgPool, QueryBuilder, Row,
};
use std::{str::FromStr, time::Duration};

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS sequences (
        owner_id BIGINT NOT NULL, environment TEXT NOT NULL, establishment TEXT NOT NULL,
        emission_point TEXT NOT NULL, last_value BIGINT NOT NULL,
        PRIMARY KEY (owner_id, environment, establishment, emission_point))",
    "CREATE TABLE IF NOT EXISTS config (
        owner_id BIGINT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL,
        PRIMARY KEY (owner_id, key))",
    "CREATE TABLE IF NOT EXISTS certificate (
        owner_id BIGINT PRIMARY KEY, p12_encrypted BYTEA NOT NULL, password_encrypted BYTEA NOT NULL)",
    "CREATE TABLE IF NOT EXISTS products (
        id BIGSERIAL PRIMARY KEY, owner_id BIGINT NOT NULL, main_code TEXT NOT NULL, aux_code TEXT,
        name TEXT NOT NULL, price BIGINT NOT NULL, vat TEXT NOT NULL DEFAULT '15',
        UNIQUE (owner_id, main_code))",
    "CREATE TABLE IF NOT EXISTS customers (
        id BIGSERIAL PRIMARY KEY, owner_id BIGINT NOT NULL, id_type TEXT NOT NULL,
        id_number TEXT NOT NULL, business_name TEXT NOT NULL, address TEXT, phone TEXT, email TEXT,
        UNIQUE (owner_id, id_number))",
    "CREATE TABLE IF NOT EXISTS documents (
        access_key TEXT PRIMARY KEY, owner_id BIGINT NOT NULL, sequence BIGINT NOT NULL,
        issue_date TEXT, document_type TEXT NOT NULL DEFAULT '01', id_number TEXT,
        business_name TEXT, total_amount BIGINT, status TEXT NOT NULL, signed_xml TEXT NOT NULL,
        authorization_number TEXT, authorization_date TEXT, messages TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now())",
    "CREATE INDEX IF NOT EXISTS documents_owner_idx ON documents (owner_id, issue_date)",
    "CREATE INDEX IF NOT EXISTS documents_pending_idx ON documents (status) WHERE status = 'PENDIENTE'",
    "CREATE TABLE IF NOT EXISTS users (
        id BIGSERIAL PRIMARY KEY, username TEXT NOT NULL, password_hash TEXT NOT NULL,
        role TEXT NOT NULL CHECK (role IN ('admin', 'operator')), active BOOLEAN NOT NULL DEFAULT TRUE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now())",
    "CREATE UNIQUE INDEX IF NOT EXISTS users_username_ci ON users (lower(username))",
    "CREATE TABLE IF NOT EXISTS sessions (
        token_hash TEXT PRIMARY KEY, user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        expires_at BIGINT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS throttle (
        key TEXT PRIMARY KEY, hits INTEGER NOT NULL, since BIGINT NOT NULL)",
];

pub struct PgStore {
    pool: PgPool,
}

fn be(e: sqlx::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

fn unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}

impl PgStore {
    /// Conecta y crea las tablas que falten. `url` es la cadena de conexión de Supabase.
    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        let options = PgConnectOptions::from_str(url)
            .map_err(be)?
            .statement_cache_capacity(0);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(30))
            .connect_with(options)
            .await
            .map_err(be)?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    /// Para pruebas: trabaja dentro de un esquema propio, así varias pruebas pueden usar la misma
    /// base a la vez sin pisarse.
    pub async fn connect_in_schema(url: &str, schema: &str) -> Result<Self, StoreError> {
        let admin = sqlx::PgPool::connect(url).await.map_err(be)?;
        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {schema}"))
            .execute(&admin)
            .await
            .map_err(be)?;
        admin.close().await;
        let options = PgConnectOptions::from_str(url)
            .map_err(be)?
            .statement_cache_capacity(0)
            .options([("search_path", schema)]);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(be)?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> Result<(), StoreError> {
        // Un candado evita que dos arranques simultáneos (serverless) creen las tablas a la vez.
        let mut conn = self.pool.acquire().await.map_err(be)?;
        sqlx::query("SELECT pg_advisory_lock(7013)")
            .execute(&mut *conn)
            .await
            .map_err(be)?;
        let mut result = Ok(());
        for ddl in SCHEMA {
            if let Err(e) = sqlx::query(ddl).execute(&mut *conn).await {
                result = Err(be(e));
                break;
            }
        }
        sqlx::query("SELECT pg_advisory_unlock(7013)")
            .execute(&mut *conn)
            .await
            .map_err(be)?;
        result
    }
}

fn record(r: &PgRow) -> Record {
    Record {
        access_key: r.get("access_key"),
        sequence: r.get::<i64, _>("sequence") as u32,
        issue_date: r.get("issue_date"),
        document_type: r.get("document_type"),
        id_number: r.get("id_number"),
        business_name: r.get("business_name"),
        total_amount: r.get("total_amount"),
        status: r.get("status"),
        signed_xml: r.get("signed_xml"),
        authorization_number: r.get("authorization_number"),
        authorization_date: r.get("authorization_date"),
        messages: r.get("messages"),
    }
}

const RECORD: &str = "access_key, sequence, issue_date, document_type, id_number, business_name,
    total_amount, status, signed_xml, authorization_number, authorization_date, messages";

fn user(r: &PgRow) -> User {
    User {
        id: r.get("id"),
        username: r.get("username"),
        role: Role::parse(&r.get::<String, _>("role")).unwrap_or(Role::Operator),
        active: r.get("active"),
    }
}

fn product(r: &PgRow) -> Product {
    Product {
        id: r.get("id"),
        main_code: r.get("main_code"),
        aux_code: r.get("aux_code"),
        name: r.get("name"),
        price_cents: r.get("price"),
        vat: r.get("vat"),
    }
}

fn customer(r: &PgRow) -> Customer {
    Customer {
        id: r.get("id"),
        id_type: r.get("id_type"),
        id_number: r.get("id_number"),
        business_name: r.get("business_name"),
        address: r.get("address"),
        phone: r.get("phone"),
        email: r.get("email"),
    }
}

/// Subcadena para `LIKE` con `%` y `_` literales.
fn contains(q: &str) -> String {
    format!("%{}%", escape(q))
}
fn prefix(q: &str) -> String {
    format!("{}%", escape(q))
}
fn escape(q: &str) -> String {
    q.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn catalog_err(e: sqlx::Error) -> CatalogError {
    if unique_violation(&e) {
        CatalogError::Duplicate
    } else if matches!(e, sqlx::Error::RowNotFound) {
        CatalogError::NotFound
    } else {
        CatalogError::Store(be(e))
    }
}

fn affected(n: u64) -> Result<(), CatalogError> {
    if n == 0 {
        Err(CatalogError::NotFound)
    } else {
        Ok(())
    }
}

#[async_trait]
impl Db for PgStore {
    // ---------- secuenciales y comprobantes ----------
    async fn reserve_sequence(
        &self,
        owner: i64,
        env: Environment,
        establishment: &str,
        emission_point: &str,
    ) -> Result<u32, StoreError> {
        let n: i64 = sqlx::query_scalar(
            "INSERT INTO sequences (owner_id, environment, establishment, emission_point, last_value)
             VALUES ($1, $2, $3, $4, 1)
             ON CONFLICT (owner_id, environment, establishment, emission_point)
             DO UPDATE SET last_value = sequences.last_value + 1
             RETURNING last_value",
        )
        .bind(owner)
        .bind(env_label(env))
        .bind(establishment)
        .bind(emission_point)
        .fetch_one(&self.pool)
        .await
        .map_err(be)?;
        Ok(n as u32)
    }

    async fn save_pending(
        &self,
        owner: i64,
        access_key: &str,
        sequence: u32,
        signed_xml: &str,
        m: &Meta,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO documents (owner_id, access_key, sequence, status, signed_xml, issue_date,
                document_type, id_number, business_name, total_amount)
             VALUES ($1, $2, $3, 'PENDIENTE', $4, $5, $6, $7, $8, $9)",
        )
        .bind(owner)
        .bind(access_key)
        .bind(sequence as i64)
        .bind(signed_xml)
        .bind(&m.issue_date)
        .bind(&m.document_type)
        .bind(&m.id_number)
        .bind(&m.business_name)
        .bind(m.total_amount)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn update_status(
        &self,
        access_key: &str,
        status: &str,
        number: Option<&str>,
        date: Option<&str>,
        messages: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE documents SET status=$2, authorization_number=$3, authorization_date=$4, messages=$5
             WHERE access_key=$1",
        )
        .bind(access_key)
        .bind(status)
        .bind(number)
        .bind(date)
        .bind(messages)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn get_record(&self, owner: i64, access_key: &str) -> Result<Option<Record>, StoreError> {
        let row = sqlx::query(&format!(
            "SELECT {RECORD} FROM documents WHERE access_key=$1 AND owner_id=$2"
        ))
        .bind(access_key)
        .bind(owner)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        Ok(row.as_ref().map(record))
    }

    async fn get_record_any(&self, access_key: &str) -> Result<Option<Record>, StoreError> {
        let row = sqlx::query(&format!(
            "SELECT {RECORD} FROM documents WHERE access_key=$1"
        ))
        .bind(access_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        Ok(row.as_ref().map(record))
    }

    async fn search(&self, owner: i64, f: &Filter) -> Result<(Vec<Record>, u64), StoreError> {
        fn conditions<'a>(q: &mut QueryBuilder<'a, sqlx::Postgres>, owner: i64, f: &'a Filter) {
            q.push(" WHERE owner_id = ").push_bind(owner);
            if let Some(v) = &f.date_from {
                q.push(" AND issue_date >= ").push_bind(v);
            }
            if let Some(v) = &f.date_to {
                q.push(" AND issue_date <= ").push_bind(v);
            }
            if let Some(v) = &f.id_number {
                q.push(" AND id_number LIKE ")
                    .push_bind(prefix(v))
                    .push(" ESCAPE '\\'");
            }
            if let Some(v) = &f.document_type {
                q.push(" AND document_type = ").push_bind(v);
            }
            if let Some(v) = &f.status {
                q.push(" AND status = ").push_bind(v);
            }
            if let Some(v) = &f.access_key {
                q.push(" AND (access_key LIKE ")
                    .push_bind(prefix(v))
                    .push(" ESCAPE '\\' OR authorization_number LIKE ")
                    .push_bind(prefix(v))
                    .push(" ESCAPE '\\')");
            }
        }
        let mut count = QueryBuilder::new("SELECT COUNT(*) FROM documents");
        conditions(&mut count, owner, f);
        let total: i64 = count
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(be)?;

        let mut q = QueryBuilder::new(
            "SELECT access_key, sequence, issue_date, document_type, id_number, business_name,
                    total_amount, status, ''::text AS signed_xml, authorization_number,
                    authorization_date, messages FROM documents",
        );
        conditions(&mut q, owner, f);
        q.push(" ORDER BY COALESCE(issue_date, '') DESC, sequence DESC LIMIT ")
            .push_bind(f.limit.unwrap_or(50).min(10_000) as i64)
            .push(" OFFSET ")
            .push_bind(f.offset.unwrap_or(0) as i64);
        let rows = q.build().fetch_all(&self.pool).await.map_err(be)?;
        Ok((rows.iter().map(record).collect(), total as u64))
    }

    async fn pending(&self, owner: Option<i64>) -> Result<Vec<Record>, StoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {RECORD} FROM documents
             WHERE status='PENDIENTE' AND ($1::bigint IS NULL OR owner_id = $1)
             ORDER BY owner_id, sequence"
        ))
        .bind(owner)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        Ok(rows.iter().map(record).collect())
    }

    // ---------- configuración ----------
    async fn save_issuer(
        &self,
        owner: i64,
        e: &Issuer,
        env: Environment,
    ) -> Result<(), StoreError> {
        let json = serde_json::to_string(e).expect("serialize issuer");
        for (k, v) in [
            ("issuer", json.as_str()),
            ("environment", environment_text(env)),
        ] {
            sqlx::query(
                "INSERT INTO config (owner_id, key, value) VALUES ($1, $2, $3)
                 ON CONFLICT (owner_id, key) DO UPDATE SET value = excluded.value",
            )
            .bind(owner)
            .bind(k)
            .bind(v)
            .execute(&self.pool)
            .await
            .map_err(be)?;
        }
        Ok(())
    }

    async fn load_issuer(&self, owner: i64) -> Result<Option<(Issuer, Environment)>, StoreError> {
        let rows = sqlx::query("SELECT key, value FROM config WHERE owner_id = $1")
            .bind(owner)
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
        let get = |k: &str| {
            rows.iter()
                .find(|r| r.get::<String, _>("key") == k)
                .map(|r| r.get::<String, _>("value"))
        };
        let Some(json) = get("issuer") else {
            return Ok(None);
        };
        let issuer: Issuer =
            serde_json::from_str(&json).map_err(|e| StoreError::Backend(e.to_string()))?;
        let env = get("environment")
            .and_then(|a| parse_environment(&a))
            .unwrap_or(Environment::Testing);
        Ok(Some((issuer, env)))
    }

    async fn save_certificate(
        &self,
        owner: i64,
        key: &EncryptionKey,
        p12: &[u8],
        password: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO certificate (owner_id, p12_encrypted, password_encrypted) VALUES ($1, $2, $3)
             ON CONFLICT (owner_id) DO UPDATE SET p12_encrypted = excluded.p12_encrypted,
                                                  password_encrypted = excluded.password_encrypted",
        )
        .bind(owner)
        .bind(crypto::encrypt(key, p12))
        .bind(crypto::encrypt(key, password.as_bytes()))
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn load_certificate(
        &self,
        owner: i64,
        key: &EncryptionKey,
    ) -> Result<Option<(Vec<u8>, String)>, String> {
        let row = sqlx::query(
            "SELECT p12_encrypted, password_encrypted FROM certificate WHERE owner_id = $1",
        )
        .bind(owner)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let Some(row) = row else {
            return Ok(None);
        };
        let p12 = crypto::decrypt(key, &row.get::<Vec<u8>, _>("p12_encrypted"))?;
        let pass = String::from_utf8(crypto::decrypt(
            key,
            &row.get::<Vec<u8>, _>("password_encrypted"),
        )?)
        .map_err(|e| e.to_string())?;
        Ok(Some((p12, pass)))
    }

    // ---------- catálogos ----------
    async fn create_product(&self, owner: i64, p: &Product) -> Result<i64, CatalogError> {
        sqlx::query_scalar(
            "INSERT INTO products (owner_id, main_code, aux_code, name, price, vat)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(owner)
        .bind(&p.main_code)
        .bind(&p.aux_code)
        .bind(&p.name)
        .bind(p.price_cents)
        .bind(&p.vat)
        .fetch_one(&self.pool)
        .await
        .map_err(catalog_err)
    }

    async fn update_product(&self, owner: i64, id: i64, p: &Product) -> Result<(), CatalogError> {
        let n = sqlx::query(
            "UPDATE products SET main_code=$3, aux_code=$4, name=$5, price=$6, vat=$7
             WHERE id=$1 AND owner_id=$2",
        )
        .bind(id)
        .bind(owner)
        .bind(&p.main_code)
        .bind(&p.aux_code)
        .bind(&p.name)
        .bind(p.price_cents)
        .bind(&p.vat)
        .execute(&self.pool)
        .await
        .map_err(catalog_err)?
        .rows_affected();
        affected(n)
    }

    async fn delete_product(&self, owner: i64, id: i64) -> Result<(), CatalogError> {
        let n = sqlx::query("DELETE FROM products WHERE id=$1 AND owner_id=$2")
            .bind(id)
            .bind(owner)
            .execute(&self.pool)
            .await
            .map_err(catalog_err)?
            .rows_affected();
        affected(n)
    }

    async fn get_product(&self, owner: i64, id: i64) -> Result<Product, CatalogError> {
        let row = sqlx::query(
            "SELECT id, main_code, aux_code, name, price, vat FROM products WHERE id=$1 AND owner_id=$2",
        )
        .bind(id)
        .bind(owner)
        .fetch_one(&self.pool)
        .await
        .map_err(catalog_err)?;
        Ok(product(&row))
    }

    async fn search_products(
        &self,
        owner: i64,
        q: Option<&str>,
        limit: u32,
        page: u32,
    ) -> Result<(Vec<Product>, u64), CatalogError> {
        let q = q.map(str::trim).filter(|q| !q.is_empty()).map(contains);
        let limit = limit.clamp(1, 1000) as i64;
        let offset = (page.max(1) - 1) as i64 * limit;
        let filter = |b: &mut QueryBuilder<'_, sqlx::Postgres>| {
            b.push(" WHERE owner_id = ").push_bind(owner);
            if let Some(p) = &q {
                b.push(" AND (main_code ILIKE ")
                    .push_bind(p.clone())
                    .push(" ESCAPE '\\'")
                    .push(" OR aux_code ILIKE ")
                    .push_bind(p.clone())
                    .push(" ESCAPE '\\'")
                    .push(" OR name ILIKE ")
                    .push_bind(p.clone())
                    .push(" ESCAPE '\\')");
            }
        };
        let mut c = QueryBuilder::new("SELECT COUNT(*) FROM products");
        filter(&mut c);
        let total: i64 = c
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(catalog_err)?;
        let mut b =
            QueryBuilder::new("SELECT id, main_code, aux_code, name, price, vat FROM products");
        filter(&mut b);
        b.push(" ORDER BY lower(name) LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let rows = b.build().fetch_all(&self.pool).await.map_err(catalog_err)?;
        Ok((rows.iter().map(product).collect(), total as u64))
    }

    async fn create_customer(&self, owner: i64, k: &Customer) -> Result<i64, CatalogError> {
        sqlx::query_scalar(
            "INSERT INTO customers (owner_id, id_type, id_number, business_name, address, phone, email)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(owner)
        .bind(&k.id_type)
        .bind(&k.id_number)
        .bind(&k.business_name)
        .bind(&k.address)
        .bind(&k.phone)
        .bind(&k.email)
        .fetch_one(&self.pool)
        .await
        .map_err(catalog_err)
    }

    async fn update_customer(&self, owner: i64, id: i64, k: &Customer) -> Result<(), CatalogError> {
        let n = sqlx::query(
            "UPDATE customers SET id_type=$3, id_number=$4, business_name=$5, address=$6, phone=$7, email=$8
             WHERE id=$1 AND owner_id=$2",
        )
        .bind(id)
        .bind(owner)
        .bind(&k.id_type)
        .bind(&k.id_number)
        .bind(&k.business_name)
        .bind(&k.address)
        .bind(&k.phone)
        .bind(&k.email)
        .execute(&self.pool)
        .await
        .map_err(catalog_err)?
        .rows_affected();
        affected(n)
    }

    async fn delete_customer(&self, owner: i64, id: i64) -> Result<(), CatalogError> {
        let n = sqlx::query("DELETE FROM customers WHERE id=$1 AND owner_id=$2")
            .bind(id)
            .bind(owner)
            .execute(&self.pool)
            .await
            .map_err(catalog_err)?
            .rows_affected();
        affected(n)
    }

    async fn get_customer(&self, owner: i64, id: i64) -> Result<Customer, CatalogError> {
        let row = sqlx::query(
            "SELECT id, id_type, id_number, business_name, address, phone, email
             FROM customers WHERE id=$1 AND owner_id=$2",
        )
        .bind(id)
        .bind(owner)
        .fetch_one(&self.pool)
        .await
        .map_err(catalog_err)?;
        Ok(customer(&row))
    }

    async fn search_customers(
        &self,
        owner: i64,
        q: Option<&str>,
        limit: u32,
        page: u32,
    ) -> Result<(Vec<Customer>, u64), CatalogError> {
        let q = q.map(str::trim).filter(|q| !q.is_empty()).map(contains);
        let limit = limit.clamp(1, 1000) as i64;
        let offset = (page.max(1) - 1) as i64 * limit;
        let filter = |b: &mut QueryBuilder<'_, sqlx::Postgres>| {
            b.push(" WHERE owner_id = ").push_bind(owner);
            if let Some(p) = &q {
                b.push(" AND (id_number ILIKE ")
                    .push_bind(p.clone())
                    .push(" ESCAPE '\\'")
                    .push(" OR business_name ILIKE ")
                    .push_bind(p.clone())
                    .push(" ESCAPE '\\')");
            }
        };
        let mut c = QueryBuilder::new("SELECT COUNT(*) FROM customers");
        filter(&mut c);
        let total: i64 = c
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(catalog_err)?;
        let mut b = QueryBuilder::new(
            "SELECT id, id_type, id_number, business_name, address, phone, email FROM customers",
        );
        filter(&mut b);
        b.push(" ORDER BY lower(business_name) LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let rows = b.build().fetch_all(&self.pool).await.map_err(catalog_err)?;
        Ok((rows.iter().map(customer).collect(), total as u64))
    }

    // ---------- cuentas y sesiones ----------
    async fn count_users(&self) -> Result<i64, AccountError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(be)?)
    }

    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: Role,
    ) -> Result<User, AccountError> {
        let username = validate_username(username)?;
        validate_password(password)?;
        let hash = hash_password(password);
        let row = sqlx::query(
            "INSERT INTO users (username, password_hash, role) VALUES ($1, $2, $3)
             RETURNING id, username, role, active",
        )
        .bind(&username)
        .bind(hash)
        .bind(role.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if unique_violation(&e) {
                AccountError::Duplicate
            } else {
                AccountError::Store(be(e))
            }
        })?;
        Ok(user(&row))
    }

    async fn list_users(&self) -> Result<Vec<User>, AccountError> {
        let rows = sqlx::query("SELECT id, username, role, active FROM users ORDER BY username")
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
        Ok(rows.iter().map(user).collect())
    }

    async fn find_login(&self, username: &str) -> Result<Option<(User, String)>, AccountError> {
        let row = sqlx::query(
            "SELECT id, username, role, active, password_hash FROM users WHERE lower(username) = lower($1)",
        )
        .bind(username.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        Ok(row.map(|r| (user(&r), r.get("password_hash"))))
    }

    async fn password_hash_of(&self, id: i64) -> Result<Option<String>, AccountError> {
        Ok(
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(be)?,
        )
    }

    async fn update_user(
        &self,
        id: i64,
        role: Option<Role>,
        active: Option<bool>,
        password: Option<&str>,
    ) -> Result<User, AccountError> {
        if let Some(p) = password {
            validate_password(p)?;
        }
        let mut tx = self.pool.begin().await.map_err(be)?;
        let current =
            sqlx::query("SELECT id, username, role, active FROM users WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(be)?
                .map(|r| user(&r))
                .ok_or(AccountError::NotFound)?;
        let new_role = role.unwrap_or(current.role);
        let new_active = active.unwrap_or(current.active);
        let was_active_admin = current.role == Role::Admin && current.active;
        let stays_active_admin = new_role == Role::Admin && new_active;
        if was_active_admin && !stays_active_admin {
            let admins: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin' AND active")
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(be)?;
            if admins <= 1 {
                return Err(AccountError::LastAdmin);
            }
        }
        sqlx::query("UPDATE users SET role = $1, active = $2 WHERE id = $3")
            .bind(new_role.as_str())
            .bind(new_active)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(be)?;
        if let Some(p) = password {
            sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
                .bind(hash_password(p))
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(be)?;
        }
        if password.is_some() || !new_active || new_role != current.role {
            sqlx::query("DELETE FROM sessions WHERE user_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(be)?;
        }
        tx.commit().await.map_err(be)?;
        Ok(User {
            id,
            username: current.username,
            role: new_role,
            active: new_active,
        })
    }

    async fn create_session(&self, user_id: i64) -> Result<(String, i64), AccountError> {
        let token = new_token();
        let expires = now() + SESSION_SECONDS;
        sqlx::query("DELETE FROM sessions WHERE expires_at < $1")
            .bind(now())
            .execute(&self.pool)
            .await
            .map_err(be)?;
        sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
            .bind(hash_token(&token))
            .bind(user_id)
            .bind(expires)
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok((token, expires))
    }

    async fn session_user(&self, token: &str) -> Result<Option<User>, AccountError> {
        let row = sqlx::query(
            "SELECT u.id, u.username, u.role, u.active FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.expires_at > $2 AND u.active",
        )
        .bind(hash_token(token))
        .bind(now())
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        Ok(row.as_ref().map(user))
    }

    async fn delete_session(&self, token: &str) -> Result<(), AccountError> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(hash_token(token))
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok(())
    }

    async fn delete_other_sessions(&self, user_id: i64, keep: &str) -> Result<(), AccountError> {
        sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2")
            .bind(user_id)
            .bind(hash_token(keep))
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok(())
    }

    async fn set_password(&self, user_id: i64, password: &str) -> Result<(), AccountError> {
        validate_password(password)?;
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(hash_password(password))
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok(())
    }

    // ---------- límite de intentos ----------
    async fn throttle_blocked(&self, key: &str, max: u32, window: i64) -> Result<bool, StoreError> {
        let row = sqlx::query("SELECT hits, since FROM throttle WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?;
        match row {
            Some(r) if r.get::<i64, _>("since") + window < now() => {
                self.throttle_clear(key).await?;
                Ok(false)
            }
            Some(r) => Ok(r.get::<i32, _>("hits") as i64 >= max as i64),
            None => Ok(false),
        }
    }

    async fn throttle_hit(&self, key: &str) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO throttle (key, hits, since) VALUES ($1, 1, $2)
             ON CONFLICT (key) DO UPDATE SET hits = throttle.hits + 1",
        )
        .bind(key)
        .bind(now())
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn throttle_clear(&self, key: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM throttle WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok(())
    }
}
