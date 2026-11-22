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
    /// `aaaa-mm-dd`; `None` en comprobantes guardados antes de existir esta columna.
    pub fecha_emision: Option<String>,
    /// Código de tabla 3 del SRI (`01` factura).
    pub tipo_comprobante: String,
    pub identificacion: Option<String>,
    pub razon_social: Option<String>,
    /// Total en centavos.
    pub valor_total: Option<i64>,
    /// `PENDIENTE` (firmado, sin respuesta final), `AUTORIZADO`, `RECHAZADO`, `DEVUELTA`...
    pub estado: String,
    pub xml_firmado: String,
    pub numero_autorizacion: Option<String>,
    pub fecha_autorizacion: Option<String>,
    pub mensajes: Option<String>,
}

/// Datos del comprobante que se guardan para poder buscarlo después.
#[derive(Debug, Clone)]
pub struct Meta {
    pub fecha_emision: String,
    pub tipo_comprobante: String,
    pub identificacion: String,
    pub razon_social: String,
    pub valor_total: i64,
}

/// Filtros de búsqueda (todos opcionales).
#[derive(Debug, Clone, Default)]
pub struct Filtro {
    /// `aaaa-mm-dd`, inclusive.
    pub desde: Option<String>,
    pub hasta: Option<String>,
    /// Prefijo de la identificación del comprador.
    pub identificacion: Option<String>,
    pub tipo_comprobante: Option<String>,
    pub estado: Option<String>,
    /// Prefijo de la clave de acceso / número de autorización.
    pub clave: Option<String>,
    pub limite: Option<u32>,
    pub desplazamiento: Option<u32>,
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
    fecha_emision TEXT,
    tipo_comprobante TEXT NOT NULL DEFAULT '01',
    identificacion TEXT,
    razon_social TEXT,
    valor_total INTEGER,
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
        migrar(&conn)?;
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

    pub fn guardar_pendiente(
        &self,
        clave: &str,
        secuencial: u32,
        xml_firmado: &str,
        m: &Meta,
    ) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO comprobantes (clave_acceso, secuencial, estado, xml_firmado,
                fecha_emision, tipo_comprobante, identificacion, razon_social, valor_total)
             VALUES (?1, ?2, 'PENDIENTE', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                clave,
                secuencial,
                xml_firmado,
                m.fecha_emision,
                m.tipo_comprobante,
                m.identificacion,
                m.razon_social,
                m.valor_total
            ],
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

    /// Busca comprobantes (más recientes primero), sin el XML para aligerar la respuesta.
    /// Devuelve la página pedida y el total de coincidencias.
    pub fn buscar(&self, f: &Filtro) -> Result<(Vec<Registro>, u64), StoreError> {
        let mut cond: Vec<&str> = vec![];
        let mut args: Vec<rusqlite::types::Value> = vec![];
        let mut add = |c: &'static str, v: String| {
            cond.push(c);
            args.push(v.into());
        };
        // Los `LIKE` usan ESCAPE para que `%` y `_` del usuario sean literales.
        let prefijo = |s: &str| format!("{}%", s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        if let Some(v) = &f.desde { add("fecha_emision >= ?", v.clone()); }
        if let Some(v) = &f.hasta { add("fecha_emision <= ?", v.clone()); }
        if let Some(v) = &f.identificacion { add("identificacion LIKE ? ESCAPE '\\'", prefijo(v)); }
        if let Some(v) = &f.tipo_comprobante { add("tipo_comprobante = ?", v.clone()); }
        if let Some(v) = &f.estado { add("estado = ?", v.clone()); }
        if let Some(v) = &f.clave {
            add("(clave_acceso LIKE ? ESCAPE '\\' OR numero_autorizacion LIKE ? ESCAPE '\\')", prefijo(v));
            args.push(prefijo(v).into());
        }
        let donde = if cond.is_empty() { String::new() } else { format!(" WHERE {}", cond.join(" AND ")) };

        let c = self.conn.lock().unwrap();
        let total: u64 = c.query_row(
            &format!("SELECT COUNT(*) FROM comprobantes{donde}"),
            rusqlite::params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        let limite = f.limite.unwrap_or(50).min(10_000);
        let mut args2 = args.clone();
        args2.push((limite as i64).into());
        args2.push((f.desplazamiento.unwrap_or(0) as i64).into());
        let mut st = c.prepare(&format!(
            "SELECT clave_acceso, secuencial, fecha_emision, tipo_comprobante, identificacion, razon_social,
                    valor_total, estado, '', numero_autorizacion, fecha_autorizacion, mensajes
             FROM comprobantes{donde} ORDER BY COALESCE(fecha_emision, '') DESC, secuencial DESC LIMIT ? OFFSET ?"
        ))?;
        let v = st
            .query_map(rusqlite::params_from_iter(args2.iter()), fila)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((v, total))
    }

    pub fn pendientes(&self) -> Result<Vec<Registro>, StoreError> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare(&format!("{SELECT} WHERE estado='PENDIENTE' ORDER BY secuencial"))?;
        let v = st.query_map([], fila)?.collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }
}

const SELECT: &str = "SELECT clave_acceso, secuencial, fecha_emision, tipo_comprobante, identificacion,
    razon_social, valor_total, estado, xml_firmado, numero_autorizacion, fecha_autorizacion, mensajes
    FROM comprobantes";

fn fila(r: &rusqlite::Row) -> rusqlite::Result<Registro> {
    Ok(Registro {
        clave_acceso: r.get(0)?,
        secuencial: r.get(1)?,
        fecha_emision: r.get(2)?,
        tipo_comprobante: r.get(3)?,
        identificacion: r.get(4)?,
        razon_social: r.get(5)?,
        valor_total: r.get(6)?,
        estado: r.get(7)?,
        xml_firmado: r.get(8)?,
        numero_autorizacion: r.get(9)?,
        fecha_autorizacion: r.get(10)?,
        mensajes: r.get(11)?,
    })
}

/// Agrega las columnas nuevas a bases creadas por versiones anteriores.
fn migrar(conn: &Connection) -> Result<(), StoreError> {
    let existentes: Vec<String> = conn
        .prepare("PRAGMA table_info(comprobantes)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    for (col, def) in [
        ("fecha_emision", "TEXT"),
        ("tipo_comprobante", "TEXT NOT NULL DEFAULT '01'"),
        ("identificacion", "TEXT"),
        ("razon_social", "TEXT"),
        ("valor_total", "INTEGER"),
    ] {
        if !existentes.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE comprobantes ADD COLUMN {col} {def}"), [])?;
        }
    }
    Ok(())
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

    fn meta(fecha: &str, ident: &str, total: i64) -> Meta {
        Meta {
            fecha_emision: fecha.into(),
            tipo_comprobante: "01".into(),
            identificacion: ident.into(),
            razon_social: "CLIENTE".into(),
            valor_total: total,
        }
    }

    #[test]
    fn ciclo_de_vida_del_comprobante() {
        let s = Store::en_memoria().unwrap();
        s.guardar_pendiente("K1", 1, "<x/>", &meta("2026-11-20", "1712345678", 1725)).unwrap();
        assert_eq!(s.pendientes().unwrap().len(), 1);
        s.actualizar_estado("K1", "AUTORIZADO", Some("K1"), Some("2026-11-22"), None).unwrap();
        let r = s.obtener("K1").unwrap().unwrap();
        assert_eq!(r.estado, "AUTORIZADO");
        assert_eq!(r.valor_total, Some(1725));
        assert_eq!(r.numero_autorizacion.as_deref(), Some("K1"));
        assert!(s.pendientes().unwrap().is_empty());
        assert!(s.obtener("nada").unwrap().is_none());
        assert!(s.guardar_pendiente("K1", 2, "<y/>", &meta("2026-11-20", "1", 1)).is_err());
    }

    #[test]
    fn busqueda_con_filtros_y_paginacion() {
        let s = Store::en_memoria().unwrap();
        s.guardar_pendiente("A1", 1, "<x/>", &meta("2026-11-20", "1712345678", 100)).unwrap();
        s.guardar_pendiente("A2", 2, "<x/>", &meta("2026-11-21", "9999999999999", 200)).unwrap();
        s.guardar_pendiente("B3", 3, "<x/>", &meta("2026-11-22", "1712345678", 300)).unwrap();
        s.actualizar_estado("B3", "AUTORIZADO", Some("NUM9"), None, None).unwrap();

        let (todos, n) = s.buscar(&Filtro::default()).unwrap();
        assert_eq!(n, 3);
        assert_eq!(todos[0].clave_acceso, "B3"); // más reciente primero
        assert!(todos.iter().all(|r| r.xml_firmado.is_empty()));

        let f = |f: Filtro| s.buscar(&f).unwrap();
        let (v, n) = f(Filtro { desde: Some("2026-11-21".into()), hasta: Some("2026-11-21".into()), ..Default::default() });
        assert_eq!((n, v[0].clave_acceso.as_str()), (1, "A2"));
        assert_eq!(f(Filtro { identificacion: Some("17123".into()), ..Default::default() }).1, 2);
        assert_eq!(f(Filtro { estado: Some("AUTORIZADO".into()), ..Default::default() }).1, 1);
        assert_eq!(f(Filtro { clave: Some("NUM".into()), ..Default::default() }).1, 1); // por número de autorización
        assert_eq!(f(Filtro { clave: Some("A".into()), ..Default::default() }).1, 2);
        // `%` y `_` se tratan literalmente.
        assert_eq!(f(Filtro { identificacion: Some("%".into()), ..Default::default() }).1, 0);
        let (pag, total) = f(Filtro { limite: Some(2), desplazamiento: Some(2), ..Default::default() });
        assert_eq!((pag.len(), total), (1, 3));
    }

    #[test]
    fn migra_base_antigua() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE comprobantes (clave_acceso TEXT PRIMARY KEY, secuencial INTEGER NOT NULL,
             estado TEXT NOT NULL, xml_firmado TEXT NOT NULL, numero_autorizacion TEXT,
             fecha_autorizacion TEXT, mensajes TEXT, creado TEXT NOT NULL DEFAULT (datetime('now')));
             INSERT INTO comprobantes (clave_acceso, secuencial, estado, xml_firmado) VALUES ('OLD', 1, 'AUTORIZADO', '<x/>');",
        )
        .unwrap();
        let s = Store::desde(c).unwrap();
        let r = s.obtener("OLD").unwrap().unwrap();
        assert_eq!((r.fecha_emision, r.valor_total, r.tipo_comprobante.as_str()), (None, None, "01"));
        assert_eq!(s.buscar(&Filtro::default()).unwrap().1, 1);
    }
}
