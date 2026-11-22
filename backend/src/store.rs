//! Persistencia en SQLite: secuenciales y comprobantes emitidos.

use crate::clave_acceso::Ambiente;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("base de datos: {0}")]
    Db(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registro {
    pub clave_acceso: String,
    pub secuencial: u32,
    /// `PENDIENTE` (firmado, sin respuesta final), `AUTORIZADO`, `RECHAZADO`, `DEVUELTA`...
    pub estado: String,
    pub xml_firmado: String,
    pub numero_autorizacion: Option<String>,
    pub fecha_autorizacion: Option<String>,
    pub mensajes: Option<String>,
}

pub struct Store {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS secuenciales (
    ambiente TEXT NOT NULL, estab TEXT NOT NULL, pto_emi TEXT NOT NULL,
    ultimo INTEGER NOT NULL,
    PRIMARY KEY (ambiente, estab, pto_emi)
);
CREATE TABLE IF NOT EXISTS comprobantes (
    clave_acceso TEXT PRIMARY KEY,
    secuencial INTEGER NOT NULL,
    estado TEXT NOT NULL,
    xml_firmado TEXT NOT NULL,
    numero_autorizacion TEXT,
    fecha_autorizacion TEXT,
    mensajes TEXT,
    creado TEXT NOT NULL DEFAULT (datetime('now'))
);";

fn amb(a: Ambiente) -> &'static str {
    match a {
        Ambiente::Pruebas => "PRUEBAS",
        Ambiente::Produccion => "PRODUCCION",
    }
}

impl Store {
    pub fn abrir(ruta: &str) -> Result<Self, StoreError> {
        Self::desde(Connection::open(ruta)?)
    }

    pub fn en_memoria() -> Result<Self, StoreError> {
        Self::desde(Connection::open_in_memory()?)
    }

    fn desde(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Reserva el siguiente secuencial de forma atómica. Un número reservado no se
    /// reutiliza aunque la emisión falle después.
    pub fn reservar_secuencial(&self, a: Ambiente, estab: &str, pto_emi: &str) -> Result<u32, StoreError> {
        let c = self.conn.lock().unwrap();
        let n = c.query_row(
            "INSERT INTO secuenciales (ambiente, estab, pto_emi, ultimo) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT (ambiente, estab, pto_emi) DO UPDATE SET ultimo = ultimo + 1
             RETURNING ultimo",
            params![amb(a), estab, pto_emi],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// Fija el último secuencial usado (p. ej. al migrar desde otro sistema). Nunca lo
    /// baja: si ya hay uno mayor, se conserva, para no repetir números.
    pub fn fijar_secuencial(&self, a: Ambiente, estab: &str, pto_emi: &str, ultimo: u32) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO secuenciales (ambiente, estab, pto_emi, ultimo) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (ambiente, estab, pto_emi) DO UPDATE SET ultimo = MAX(ultimo, excluded.ultimo)",
            params![amb(a), estab, pto_emi, ultimo],
        )?;
        Ok(())
    }

    pub fn guardar_pendiente(&self, clave: &str, secuencial: u32, xml_firmado: &str) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO comprobantes (clave_acceso, secuencial, estado, xml_firmado)
             VALUES (?1, ?2, 'PENDIENTE', ?3)",
            params![clave, secuencial, xml_firmado],
        )?;
        Ok(())
    }

    pub fn actualizar_estado(
        &self,
        clave: &str,
        estado: &str,
        numero: Option<&str>,
        fecha: Option<&str>,
        mensajes: Option<&str>,
    ) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "UPDATE comprobantes SET estado=?2, numero_autorizacion=?3, fecha_autorizacion=?4, mensajes=?5
             WHERE clave_acceso=?1",
            params![clave, estado, numero, fecha, mensajes],
        )?;
        Ok(())
    }

    pub fn obtener(&self, clave: &str) -> Result<Option<Registro>, StoreError> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .query_row(&format!("{SELECT} WHERE clave_acceso=?1"), params![clave], fila)
            .optional()?)
    }

    pub fn pendientes(&self) -> Result<Vec<Registro>, StoreError> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare(&format!("{SELECT} WHERE estado='PENDIENTE' ORDER BY secuencial"))?;
        let v = st.query_map([], fila)?.collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }
}

const SELECT: &str = "SELECT clave_acceso, secuencial, estado, xml_firmado, numero_autorizacion,
    fecha_autorizacion, mensajes FROM comprobantes";

fn fila(r: &rusqlite::Row) -> rusqlite::Result<Registro> {
    Ok(Registro {
        clave_acceso: r.get(0)?,
        secuencial: r.get(1)?,
        estado: r.get(2)?,
        xml_firmado: r.get(3)?,
        numero_autorizacion: r.get(4)?,
        fecha_autorizacion: r.get(5)?,
        mensajes: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secuenciales_incrementan_por_serie_y_ambiente() {
        let s = Store::en_memoria().unwrap();
        let p = Ambiente::Pruebas;
        assert_eq!(s.reservar_secuencial(p, "001", "001").unwrap(), 1);
        assert_eq!(s.reservar_secuencial(p, "001", "001").unwrap(), 2);
        assert_eq!(s.reservar_secuencial(p, "001", "002").unwrap(), 1);
        assert_eq!(s.reservar_secuencial(Ambiente::Produccion, "001", "001").unwrap(), 1);
        s.fijar_secuencial(p, "001", "001", 100).unwrap();
        assert_eq!(s.reservar_secuencial(p, "001", "001").unwrap(), 101);
        s.fijar_secuencial(p, "001", "001", 5).unwrap(); // no debe bajar
        assert_eq!(s.reservar_secuencial(p, "001", "001").unwrap(), 102);
    }

    #[test]
    fn ciclo_de_vida_del_comprobante() {
        let s = Store::en_memoria().unwrap();
        s.guardar_pendiente("K1", 1, "<x/>").unwrap();
        assert_eq!(s.pendientes().unwrap().len(), 1);
        s.actualizar_estado("K1", "AUTORIZADO", Some("K1"), Some("2026-11-22"), None).unwrap();
        let r = s.obtener("K1").unwrap().unwrap();
        assert_eq!(r.estado, "AUTORIZADO");
        assert_eq!(r.numero_autorizacion.as_deref(), Some("K1"));
        assert!(s.pendientes().unwrap().is_empty());
        assert!(s.obtener("nada").unwrap().is_none());
        assert!(s.guardar_pendiente("K1", 2, "<y/>").is_err());
    }
}
