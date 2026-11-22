//! La interfaz de almacenamiento. Hay dos motores: SQLite (escritorio y servidor simple, un
//! archivo local) y Postgres (por ejemplo Supabase, para despliegues sin disco como Vercel).
//! El resto del sistema solo conoce este trait.

use crate::{
    access_key::Environment,
    accounts::{AccountError, Role, User},
    catalog::{CatalogError, Customer, Product},
    crypto::EncryptionKey,
    invoice::Issuer,
    store::{Filter, Meta, Record, Store, StoreError},
};
use async_trait::async_trait;

#[async_trait]
pub trait Db: Send + Sync {
    // ---------- secuenciales y comprobantes ----------
    async fn reserve_sequence(
        &self,
        owner: i64,
        env: Environment,
        establishment: &str,
        emission_point: &str,
    ) -> Result<u32, StoreError>;
    async fn save_pending(
        &self,
        owner: i64,
        access_key: &str,
        sequence: u32,
        signed_xml: &str,
        meta: &Meta,
    ) -> Result<(), StoreError>;
    async fn update_status(
        &self,
        access_key: &str,
        status: &str,
        number: Option<&str>,
        date: Option<&str>,
        messages: Option<&str>,
    ) -> Result<(), StoreError>;
    async fn get_record(&self, owner: i64, access_key: &str) -> Result<Option<Record>, StoreError>;
    /// Por clave de acceso, de cualquier usuario (uso interno del servicio).
    async fn get_record_any(&self, access_key: &str) -> Result<Option<Record>, StoreError>;
    async fn search(&self, owner: i64, f: &Filter) -> Result<(Vec<Record>, u64), StoreError>;
    /// Pendientes de un usuario, o de todos (`None`).
    async fn pending(&self, owner: Option<i64>) -> Result<Vec<Record>, StoreError>;

    // ---------- configuración ----------
    async fn save_issuer(&self, owner: i64, e: &Issuer, env: Environment)
        -> Result<(), StoreError>;
    async fn load_issuer(&self, owner: i64) -> Result<Option<(Issuer, Environment)>, StoreError>;
    async fn save_certificate(
        &self,
        owner: i64,
        key: &EncryptionKey,
        p12: &[u8],
        password: &str,
    ) -> Result<(), StoreError>;
    async fn load_certificate(
        &self,
        owner: i64,
        key: &EncryptionKey,
    ) -> Result<Option<(Vec<u8>, String)>, String>;

    // ---------- catálogos ----------
    async fn create_product(&self, owner: i64, p: &Product) -> Result<i64, CatalogError>;
    async fn update_product(&self, owner: i64, id: i64, p: &Product) -> Result<(), CatalogError>;
    async fn delete_product(&self, owner: i64, id: i64) -> Result<(), CatalogError>;
    async fn get_product(&self, owner: i64, id: i64) -> Result<Product, CatalogError>;
    async fn search_products(
        &self,
        owner: i64,
        q: Option<&str>,
        limit: u32,
        page: u32,
    ) -> Result<(Vec<Product>, u64), CatalogError>;
    async fn create_customer(&self, owner: i64, k: &Customer) -> Result<i64, CatalogError>;
    async fn update_customer(&self, owner: i64, id: i64, k: &Customer) -> Result<(), CatalogError>;
    async fn delete_customer(&self, owner: i64, id: i64) -> Result<(), CatalogError>;
    async fn get_customer(&self, owner: i64, id: i64) -> Result<Customer, CatalogError>;
    async fn search_customers(
        &self,
        owner: i64,
        q: Option<&str>,
        limit: u32,
        page: u32,
    ) -> Result<(Vec<Customer>, u64), CatalogError>;

    // ---------- cuentas y sesiones ----------
    async fn count_users(&self) -> Result<i64, AccountError>;
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: Role,
    ) -> Result<User, AccountError>;
    async fn list_users(&self) -> Result<Vec<User>, AccountError>;
    async fn find_login(&self, username: &str) -> Result<Option<(User, String)>, AccountError>;
    async fn password_hash_of(&self, id: i64) -> Result<Option<String>, AccountError>;
    async fn update_user(
        &self,
        id: i64,
        role: Option<Role>,
        active: Option<bool>,
        password: Option<&str>,
    ) -> Result<User, AccountError>;
    async fn create_session(&self, user_id: i64) -> Result<(String, i64), AccountError>;
    async fn session_user(&self, token: &str) -> Result<Option<User>, AccountError>;
    async fn delete_session(&self, token: &str) -> Result<(), AccountError>;
    async fn delete_other_sessions(&self, user_id: i64, keep: &str) -> Result<(), AccountError>;
    async fn set_password(&self, user_id: i64, password: &str) -> Result<(), AccountError>;

    // ---------- límite de intentos (compartido entre instancias) ----------
    async fn throttle_blocked(&self, key: &str, max: u32, window: i64) -> Result<bool, StoreError>;
    async fn throttle_hit(&self, key: &str) -> Result<(), StoreError>;
    async fn throttle_clear(&self, key: &str) -> Result<(), StoreError>;
}

/// SQLite: las operaciones son cortas y locales, así que se llaman directamente.
#[async_trait]
impl Db for Store {
    async fn reserve_sequence(
        &self,
        owner: i64,
        env: Environment,
        e: &str,
        p: &str,
    ) -> Result<u32, StoreError> {
        Store::reserve_sequence(self, owner, env, e, p)
    }
    async fn save_pending(
        &self,
        owner: i64,
        k: &str,
        n: u32,
        xml: &str,
        m: &Meta,
    ) -> Result<(), StoreError> {
        Store::save_pending(self, owner, k, n, xml, m)
    }
    async fn update_status(
        &self,
        k: &str,
        s: &str,
        n: Option<&str>,
        d: Option<&str>,
        m: Option<&str>,
    ) -> Result<(), StoreError> {
        Store::update_status(self, k, s, n, d, m)
    }
    async fn get_record(&self, owner: i64, k: &str) -> Result<Option<Record>, StoreError> {
        Store::get_record(self, owner, k)
    }
    async fn get_record_any(&self, k: &str) -> Result<Option<Record>, StoreError> {
        Store::get_record_any(self, k)
    }
    async fn search(&self, owner: i64, f: &Filter) -> Result<(Vec<Record>, u64), StoreError> {
        Store::search(self, owner, f)
    }
    async fn pending(&self, owner: Option<i64>) -> Result<Vec<Record>, StoreError> {
        Store::pending(self, owner)
    }
    async fn save_issuer(&self, owner: i64, e: &Issuer, a: Environment) -> Result<(), StoreError> {
        Store::save_issuer(self, owner, e, a)
    }
    async fn load_issuer(&self, owner: i64) -> Result<Option<(Issuer, Environment)>, StoreError> {
        Store::load_issuer(self, owner)
    }
    async fn save_certificate(
        &self,
        owner: i64,
        key: &EncryptionKey,
        p12: &[u8],
        password: &str,
    ) -> Result<(), StoreError> {
        Store::save_certificate(self, owner, key, p12, password)
    }
    async fn load_certificate(
        &self,
        owner: i64,
        key: &EncryptionKey,
    ) -> Result<Option<(Vec<u8>, String)>, String> {
        Store::load_certificate(self, owner, key)
    }
    async fn create_product(&self, o: i64, p: &Product) -> Result<i64, CatalogError> {
        Store::create_product(self, o, p)
    }
    async fn update_product(&self, o: i64, id: i64, p: &Product) -> Result<(), CatalogError> {
        Store::update_product(self, o, id, p)
    }
    async fn delete_product(&self, o: i64, id: i64) -> Result<(), CatalogError> {
        Store::delete_product(self, o, id)
    }
    async fn get_product(&self, o: i64, id: i64) -> Result<Product, CatalogError> {
        Store::get_product(self, o, id)
    }
    async fn search_products(
        &self,
        o: i64,
        q: Option<&str>,
        limit: u32,
        page: u32,
    ) -> Result<(Vec<Product>, u64), CatalogError> {
        Store::search_products(self, o, q, limit, page)
    }
    async fn create_customer(&self, o: i64, k: &Customer) -> Result<i64, CatalogError> {
        Store::create_customer(self, o, k)
    }
    async fn update_customer(&self, o: i64, id: i64, k: &Customer) -> Result<(), CatalogError> {
        Store::update_customer(self, o, id, k)
    }
    async fn delete_customer(&self, o: i64, id: i64) -> Result<(), CatalogError> {
        Store::delete_customer(self, o, id)
    }
    async fn get_customer(&self, o: i64, id: i64) -> Result<Customer, CatalogError> {
        Store::get_customer(self, o, id)
    }
    async fn search_customers(
        &self,
        o: i64,
        q: Option<&str>,
        limit: u32,
        page: u32,
    ) -> Result<(Vec<Customer>, u64), CatalogError> {
        Store::search_customers(self, o, q, limit, page)
    }
    async fn count_users(&self) -> Result<i64, AccountError> {
        Store::count_users(self)
    }
    async fn create_user(&self, u: &str, p: &str, r: Role) -> Result<User, AccountError> {
        Store::create_user(self, u, p, r)
    }
    async fn list_users(&self) -> Result<Vec<User>, AccountError> {
        Store::list_users(self)
    }
    async fn find_login(&self, u: &str) -> Result<Option<(User, String)>, AccountError> {
        Store::find_login(self, u)
    }
    async fn password_hash_of(&self, id: i64) -> Result<Option<String>, AccountError> {
        Store::password_hash_of(self, id)
    }
    async fn update_user(
        &self,
        id: i64,
        role: Option<Role>,
        active: Option<bool>,
        password: Option<&str>,
    ) -> Result<User, AccountError> {
        Store::update_user(self, id, role, active, password)
    }
    async fn create_session(&self, id: i64) -> Result<(String, i64), AccountError> {
        Store::create_session(self, id)
    }
    async fn session_user(&self, t: &str) -> Result<Option<User>, AccountError> {
        Store::session_user(self, t)
    }
    async fn delete_session(&self, t: &str) -> Result<(), AccountError> {
        Store::delete_session(self, t)
    }
    async fn delete_other_sessions(&self, id: i64, keep: &str) -> Result<(), AccountError> {
        Store::delete_other_sessions(self, id, keep)
    }
    async fn set_password(&self, id: i64, p: &str) -> Result<(), AccountError> {
        Store::set_password(self, id, p)
    }
    async fn throttle_blocked(&self, k: &str, max: u32, w: i64) -> Result<bool, StoreError> {
        Store::throttle_blocked(self, k, max, w)
    }
    async fn throttle_hit(&self, k: &str) -> Result<(), StoreError> {
        Store::throttle_hit(self, k)
    }
    async fn throttle_clear(&self, k: &str) -> Result<(), StoreError> {
        Store::throttle_clear(self, k)
    }
}
