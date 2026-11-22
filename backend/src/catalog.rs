//! Catálogos editables desde la interfaz: productos/servicios y clientes.

use crate::store::{Store, StoreError};
use rusqlite::{params, ErrorCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Product {
    #[serde(default)]
    pub id: i64,
    pub main_code: String,
    pub aux_code: Option<String>,
    pub name: String,
    /// Precio unitario en centavos.
    pub price_cents: i64,
    /// `15`, `5`, `0`, `not_subject` o `exempt`.
    pub vat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Customer {
    #[serde(default)]
    pub id: i64,
    pub id_type: String,
    pub id_number: String,
    pub business_name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("ya existe un registro con ese código o identificación")]
    Duplicate,
    #[error("no encontrado")]
    NotFound,
    #[error(transparent)]
    Store(#[from] StoreError),
}

fn map_error(e: rusqlite::Error) -> CatalogError {
    match &e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation => {
            CatalogError::Duplicate
        }
        _ => CatalogError::Store(e.into()),
    }
}

/// Prefijo/subcadena para `LIKE` con `%` y `_` literales.
fn pattern(q: &str) -> String {
    format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

impl Store {
    // ---------- productos ----------
    fn product(r: &rusqlite::Row) -> rusqlite::Result<Product> {
        Ok(Product {
            id: r.get(0)?,
            main_code: r.get(1)?,
            aux_code: r.get(2)?,
            name: r.get(3)?,
            price_cents: r.get(4)?,
            vat: r.get(5)?,
        })
    }

    pub fn create_product(&self, owner: i64, p: &Product) -> Result<i64, CatalogError> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO products (owner_id, main_code, aux_code, name, price, vat) VALUES (?1,?2,?3,?4,?5,?6)",
            params![owner, p.main_code, p.aux_code, p.name, p.price_cents, p.vat],
        )
        .map_err(map_error)?;
        Ok(c.last_insert_rowid())
    }

    pub fn update_product(&self, owner: i64, id: i64, p: &Product) -> Result<(), CatalogError> {
        let n = self
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE products SET main_code=?2, aux_code=?3, name=?4, price=?5, vat=?6 WHERE id=?1 AND owner_id=?7",
                params![id, p.main_code, p.aux_code, p.name, p.price_cents, p.vat, owner],
            )
            .map_err(map_error)?;
        if n == 0 {
            Err(CatalogError::NotFound)
        } else {
            Ok(())
        }
    }

    pub fn delete_product(&self, owner: i64, id: i64) -> Result<(), CatalogError> {
        let n = self
            .conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM products WHERE id=?1 AND owner_id=?2",
                params![id, owner],
            )
            .map_err(map_error)?;
        if n == 0 {
            Err(CatalogError::NotFound)
        } else {
            Ok(())
        }
    }

    pub fn get_product(&self, owner: i64, id: i64) -> Result<Product, CatalogError> {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT id, main_code, aux_code, name, price, vat FROM products WHERE id=?1 AND owner_id=?2",
                params![id, owner],
                Self::product,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CatalogError::NotFound,
                e => map_error(e),
            })
    }

    /// Busca por código o nombre (subcadena). Devuelve la página y el total.
    pub fn search_products(
        &self,
        owner: i64,
        q: Option<&str>,
        limit: u32,
        page: u32,
    ) -> Result<(Vec<Product>, u64), CatalogError> {
        let c = self.conn.lock().unwrap();
        let q = q.map(str::trim).filter(|q| !q.is_empty()).map(pattern);
        let where_clause = if q.is_some() {
            " WHERE owner_id = ?1 AND (main_code LIKE ?2 ESCAPE '\\' OR aux_code LIKE ?2 ESCAPE '\\' OR name LIKE ?2 ESCAPE '\\')"
        } else {
            " WHERE owner_id = ?1"
        };
        let a: Vec<rusqlite::types::Value> = std::iter::once(owner.into())
            .chain(q.into_iter().map(Into::into))
            .collect();
        let total: u64 = c
            .query_row(
                &format!("SELECT COUNT(*) FROM products{where_clause}"),
                rusqlite::params_from_iter(a.iter()),
                |r| r.get(0),
            )
            .map_err(map_error)?;
        let mut st = c
            .prepare(&format!(
                "SELECT id, main_code, aux_code, name, price, vat FROM products{where_clause}
                 ORDER BY name COLLATE NOCASE LIMIT {} OFFSET {}",
                limit.clamp(1, 1000),
                (page.max(1) - 1) as u64 * limit.clamp(1, 1000) as u64
            ))
            .map_err(map_error)?;
        let v = st
            .query_map(rusqlite::params_from_iter(a.iter()), Self::product)
            .map_err(map_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_error)?;
        Ok((v, total))
    }

    // ---------- clientes ----------
    fn customer(r: &rusqlite::Row) -> rusqlite::Result<Customer> {
        Ok(Customer {
            id: r.get(0)?,
            id_type: r.get(1)?,
            id_number: r.get(2)?,
            business_name: r.get(3)?,
            address: r.get(4)?,
            phone: r.get(5)?,
            email: r.get(6)?,
        })
    }

    pub fn create_customer(&self, owner: i64, k: &Customer) -> Result<i64, CatalogError> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO customers (owner_id, id_type, id_number, business_name, address, phone, email) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![owner, k.id_type, k.id_number, k.business_name, k.address, k.phone, k.email],
        )
        .map_err(map_error)?;
        Ok(c.last_insert_rowid())
    }

    pub fn update_customer(&self, owner: i64, id: i64, k: &Customer) -> Result<(), CatalogError> {
        let n = self
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE customers SET id_type=?2, id_number=?3, business_name=?4, address=?5, phone=?6, email=?7 WHERE id=?1 AND owner_id=?8",
                params![id, k.id_type, k.id_number, k.business_name, k.address, k.phone, k.email, owner],
            )
            .map_err(map_error)?;
        if n == 0 {
            Err(CatalogError::NotFound)
        } else {
            Ok(())
        }
    }

    pub fn delete_customer(&self, owner: i64, id: i64) -> Result<(), CatalogError> {
        let n = self
            .conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM customers WHERE id=?1 AND owner_id=?2",
                params![id, owner],
            )
            .map_err(map_error)?;
        if n == 0 {
            Err(CatalogError::NotFound)
        } else {
            Ok(())
        }
    }

    pub fn get_customer(&self, owner: i64, id: i64) -> Result<Customer, CatalogError> {
        self.conn
            .lock()
            .unwrap()
            .query_row("SELECT id, id_type, id_number, business_name, address, phone, email FROM customers WHERE id=?1 AND owner_id=?2", params![id, owner], Self::customer)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => CatalogError::NotFound,
                e => map_error(e),
            })
    }

    pub fn search_customers(
        &self,
        owner: i64,
        q: Option<&str>,
        limit: u32,
        page: u32,
    ) -> Result<(Vec<Customer>, u64), CatalogError> {
        let c = self.conn.lock().unwrap();
        let q = q.map(str::trim).filter(|q| !q.is_empty()).map(pattern);
        let where_clause = if q.is_some() {
            " WHERE owner_id = ?1 AND (id_number LIKE ?2 ESCAPE '\\' OR business_name LIKE ?2 ESCAPE '\\')"
        } else {
            " WHERE owner_id = ?1"
        };
        let a: Vec<rusqlite::types::Value> = std::iter::once(owner.into())
            .chain(q.into_iter().map(Into::into))
            .collect();
        let total: u64 = c
            .query_row(
                &format!("SELECT COUNT(*) FROM customers{where_clause}"),
                rusqlite::params_from_iter(a.iter()),
                |r| r.get(0),
            )
            .map_err(map_error)?;
        let mut st = c
            .prepare(&format!(
                "SELECT id, id_type, id_number, business_name, address, phone, email FROM customers{where_clause}
                 ORDER BY business_name COLLATE NOCASE LIMIT {} OFFSET {}",
                limit.clamp(1, 1000),
                (page.max(1) - 1) as u64 * limit.clamp(1, 1000) as u64
            ))
            .map_err(map_error)?;
        let v = st
            .query_map(rusqlite::params_from_iter(a.iter()), Self::customer)
            .map_err(map_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_error)?;
        Ok((v, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(cod: &str, name: &str) -> Product {
        Product {
            id: 0,
            main_code: cod.into(),
            aux_code: None,
            name: name.into(),
            price_cents: 1500,
            vat: "15".into(),
        }
    }

    #[test]
    fn products_crud() {
        let s = Store::in_memory().unwrap();
        let id = s.create_product(1, &product("A1", "Camiseta")).unwrap();
        assert!(matches!(
            s.create_product(1, &product("A1", "Otra")),
            Err(CatalogError::Duplicate)
        ));
        s.create_product(1, &product("B2", "Pantalón")).unwrap();
        assert_eq!(s.search_products(1, None, 50, 1).unwrap().1, 2);
        assert_eq!(
            s.search_products(1, Some("camis"), 50, 1).unwrap().0[0].main_code,
            "A1"
        );
        assert_eq!(s.search_products(1, Some("B2"), 50, 1).unwrap().1, 1);
        assert_eq!(s.search_products(1, Some("%"), 50, 1).unwrap().1, 0); // literal
        let mut p = s.get_product(1, id).unwrap();
        p.price_cents = 2000;
        s.update_product(1, id, &p).unwrap();
        assert_eq!(s.get_product(1, id).unwrap().price_cents, 2000);
        // No se puede renombrar a un código existente.
        p.main_code = "B2".into();
        assert!(matches!(
            s.update_product(1, id, &p),
            Err(CatalogError::Duplicate)
        ));
        s.delete_product(1, id).unwrap();
        assert!(matches!(
            s.delete_product(1, id),
            Err(CatalogError::NotFound)
        ));
        assert!(matches!(s.get_product(1, id), Err(CatalogError::NotFound)));
        let (page, total) = s.search_products(1, None, 1, 2).unwrap();
        assert_eq!((page.len(), total), (0, 1));
    }

    #[test]
    fn customers_crud() {
        let s = Store::in_memory().unwrap();
        let k = Customer {
            id: 0,
            id_type: "05".into(),
            id_number: "1712345678".into(),
            business_name: "Juan Pérez".into(),
            address: None,
            phone: None,
            email: Some("j@x.com".into()),
        };
        let id = s.create_customer(1, &k).unwrap();
        assert!(matches!(
            s.create_customer(1, &k),
            Err(CatalogError::Duplicate)
        ));
        assert_eq!(s.search_customers(1, Some("1712"), 50, 1).unwrap().1, 1);
        assert_eq!(s.search_customers(1, Some("perez"), 50, 1).unwrap().1, 0); // sin tilde no coincide (SQLite)
        assert_eq!(s.search_customers(1, Some("Pérez"), 50, 1).unwrap().1, 1);
        let mut k2 = s.get_customer(1, id).unwrap();
        k2.phone = Some("099".into());
        s.update_customer(1, id, &k2).unwrap();
        assert_eq!(s.get_customer(1, id).unwrap().phone.as_deref(), Some("099"));
        s.delete_customer(1, id).unwrap();
        assert!(matches!(s.get_customer(1, id), Err(CatalogError::NotFound)));
    }
}
