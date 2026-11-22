//! API HTTP (JSON) sobre `Servicio`.
//!
//! - `GET  /health`                 sin autenticación
//! - `GET  /facturas`               búsqueda: ?desde=&hasta=&identificacion=&tipo=&estado=&clave=
//!                                  &limite=&pagina= → `{ total, items }` (sin XML)
//! - `GET  /facturas/reporte.csv`   mismos filtros, en CSV
//! - `POST /facturas`               emite una factura (201 si hay resultado final, 202 si quedó PENDIENTE)
//! - `GET  /facturas/{clave}`       estado guardado + XML firmado
//! - `POST /facturas/reintentar`    reenvía las PENDIENTE
//!
//! Autenticación: `Authorization: Bearer <API_TOKEN>`. Los montos van en centavos enteros.

use crate::{
    factura::{Comprador, Iva, Linea},
    servicio::{Servicio, ServicioError, SolicitudFactura},
    store::{Filtro, Registro},
};
use axum::{
    extract::{Path, Query, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct AppState {
    pub servicio: Servicio,
    pub token: String,
}

pub fn router(state: Arc<AppState>) -> Router {
    let protegidas = Router::new()
        .route("/facturas", post(emitir).get(buscar))
        .route("/facturas/reporte.csv", get(reporte))
        .route("/facturas/reintentar", post(reintentar))
        .route("/facturas/{clave}", get(obtener))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state);
    Router::new().route("/health", get(|| async { "ok" })).merge(protegidas)
}

// ---------- autenticación ----------

fn iguales(a: &[u8], b: &[u8]) -> bool {
    // Comparación en tiempo constante respecto al contenido.
    let mut diff = (a.len() != b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

async fn auth(State(st): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| !st.token.is_empty() && iguales(t.as_bytes(), st.token.as_bytes()));
    if ok {
        next.run(req).await
    } else {
        error(StatusCode::UNAUTHORIZED, "token inválido o ausente")
    }
}

// ---------- DTOs ----------

#[derive(Deserialize)]
pub struct CompradorDto {
    pub tipo_identificacion: String,
    pub razon_social: String,
    pub identificacion: String,
    pub direccion: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
pub enum IvaDto {
    #[serde(rename = "15")]
    Quince,
    #[serde(rename = "5")]
    Cinco,
    #[serde(rename = "0")]
    Cero,
    #[serde(rename = "no_objeto")]
    NoObjeto,
    #[serde(rename = "exento")]
    Exento,
}

#[derive(Deserialize)]
pub struct LineaDto {
    pub codigo_principal: String,
    pub descripcion: String,
    pub cantidad: u32,
    pub precio_unitario_centavos: i64,
    #[serde(default)]
    pub descuento_centavos: i64,
    pub iva: IvaDto,
}

#[derive(Deserialize)]
pub struct FacturaDto {
    /// Si falta, se emite a consumidor final.
    pub comprador: Option<CompradorDto>,
    pub lineas: Vec<LineaDto>,
    /// Tabla 24 del SRI; por defecto `01` (sin utilización del sistema financiero).
    pub forma_pago: Option<String>,
    #[serde(default)]
    pub info_adicional: Vec<(String, String)>,
}

#[derive(Serialize)]
pub struct RegistroDto {
    pub clave_acceso: String,
    /// `estab-ptoEmi-secuencial`, p. ej. `001-001-000000001`.
    pub numero: String,
    pub secuencial: u32,
    pub fecha_emision: Option<String>,
    pub tipo_comprobante: String,
    pub identificacion: Option<String>,
    pub razon_social: Option<String>,
    /// Total en centavos.
    pub valor_total: Option<i64>,
    pub estado: String,
    pub numero_autorizacion: Option<String>,
    pub fecha_autorizacion: Option<String>,
    pub mensajes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xml_firmado: Option<String>,
}

impl RegistroDto {
    fn de(r: Registro, con_xml: bool) -> Self {
        Self {
            numero: numero_comprobante(&r.clave_acceso),
            clave_acceso: r.clave_acceso,
            secuencial: r.secuencial,
            fecha_emision: r.fecha_emision,
            tipo_comprobante: r.tipo_comprobante,
            identificacion: r.identificacion,
            razon_social: r.razon_social,
            valor_total: r.valor_total,
            estado: r.estado,
            numero_autorizacion: r.numero_autorizacion,
            fecha_autorizacion: r.fecha_autorizacion,
            mensajes: r.mensajes,
            xml_firmado: con_xml.then_some(r.xml_firmado),
        }
    }
}

/// La serie y el secuencial van en las posiciones 24..39 de la clave de acceso.
fn numero_comprobante(clave: &str) -> String {
    if clave.len() == 49 && clave.is_ascii() {
        format!("{}-{}-{}", &clave[24..27], &clave[27..30], &clave[30..39])
    } else {
        String::new()
    }
}

fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn error_servicio(e: ServicioError) -> Response {
    match &e {
        ServicioError::Datos(_) => error(StatusCode::BAD_REQUEST, &e.to_string()),
        ServicioError::Sri(_) => error(StatusCode::BAD_GATEWAY, &e.to_string()),
        // No se filtran detalles internos (rutas, SQL) al cliente.
        ServicioError::Store(_) | ServicioError::Firma(_) => {
            eprintln!("error interno: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "error interno")
        }
    }
}

// ---------- handlers ----------

async fn emitir(State(st): State<Arc<AppState>>, Json(f): Json<FacturaDto>) -> Response {
    let lineas = f
        .lineas
        .into_iter()
        .map(|l| Linea {
            codigo_principal: l.codigo_principal,
            descripcion: l.descripcion,
            cantidad: l.cantidad,
            precio_unitario: l.precio_unitario_centavos,
            descuento: l.descuento_centavos,
            iva: match l.iva {
                IvaDto::Quince => Iva::Quince,
                IvaDto::Cinco => Iva::Cinco,
                IvaDto::Cero => Iva::Cero,
                IvaDto::NoObjeto => Iva::NoObjeto,
                IvaDto::Exento => Iva::Exento,
            },
        })
        .collect();
    let comprador = f.comprador.map(|c| Comprador {
        tipo_identificacion: c.tipo_identificacion,
        razon_social: c.razon_social,
        identificacion: c.identificacion,
        direccion: c.direccion,
    });
    let solicitud = SolicitudFactura {
        comprador: comprador.unwrap_or_else(Comprador::consumidor_final),
        lineas,
        forma_pago: f.forma_pago.unwrap_or_else(|| "01".into()),
        info_adicional: f.info_adicional,
    };
    match st.servicio.emitir(solicitud).await {
        Ok(r) => {
            let status = if r.estado == "PENDIENTE" { StatusCode::ACCEPTED } else { StatusCode::CREATED };
            (status, Json(RegistroDto::de(r, false))).into_response()
        }
        Err(e) => error_servicio(e),
    }
}

async fn obtener(State(st): State<Arc<AppState>>, Path(clave): Path<String>) -> Response {
    match st.servicio.store.obtener(&clave) {
        Ok(Some(r)) => Json(RegistroDto::de(r, true)).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "comprobante no encontrado"),
        Err(e) => error_servicio(e.into()),
    }
}

async fn reintentar(State(st): State<Arc<AppState>>) -> Response {
    match st.servicio.reintentar_pendientes().await {
        Ok(v) => {
            let out: Vec<_> = v
                .into_iter()
                .map(|(clave, r)| match r {
                    Ok(reg) => serde_json::json!({ "clave_acceso": clave, "estado": reg.estado }),
                    Err(e) => serde_json::json!({ "clave_acceso": clave, "error": e.to_string() }),
                })
                .collect();
            Json(out).into_response()
        }
        Err(e) => error_servicio(e),
    }
}

#[derive(Deserialize, Default)]
pub struct BusquedaQuery {
    pub desde: Option<String>,
    pub hasta: Option<String>,
    pub identificacion: Option<String>,
    pub tipo: Option<String>,
    pub estado: Option<String>,
    pub clave: Option<String>,
    pub limite: Option<u32>,
    /// Empieza en 1.
    pub pagina: Option<u32>,
}

fn fecha_ok(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn filtro(q: BusquedaQuery) -> Result<Filtro, Response> {
    let vacio = |o: Option<String>| o.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let desde = vacio(q.desde);
    let hasta = vacio(q.hasta);
    for f in [&desde, &hasta].into_iter().flatten() {
        if !fecha_ok(f) {
            return Err(error(StatusCode::BAD_REQUEST, "las fechas deben tener formato aaaa-mm-dd"));
        }
    }
    let limite = q.limite.unwrap_or(50).clamp(1, 10_000);
    Ok(Filtro {
        desde,
        hasta,
        identificacion: vacio(q.identificacion),
        tipo_comprobante: vacio(q.tipo),
        estado: vacio(q.estado),
        clave: vacio(q.clave),
        limite: Some(limite),
        desplazamiento: Some(q.pagina.unwrap_or(1).max(1).saturating_sub(1).saturating_mul(limite)),
    })
}

async fn buscar(State(st): State<Arc<AppState>>, Query(q): Query<BusquedaQuery>) -> Response {
    let f = match filtro(q) {
        Ok(f) => f,
        Err(r) => return r,
    };
    match st.servicio.store.buscar(&f) {
        Ok((v, total)) => Json(serde_json::json!({
            "total": total,
            "items": v.into_iter().map(|r| RegistroDto::de(r, false)).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => error_servicio(e.into()),
    }
}

/// Celda CSV: se entrecomilla y se neutralizan fórmulas (`=`, `+`, `-`, `@`) para que
/// abrir el reporte en una hoja de cálculo no ejecute contenido del comprador.
fn celda(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    format!("\"{}\"", s.replace('"', "\"\""))
}

async fn reporte(State(st): State<Arc<AppState>>, Query(mut q): Query<BusquedaQuery>) -> Response {
    q.limite = Some(10_000);
    q.pagina = Some(1);
    let f = match filtro(q) {
        Ok(f) => f,
        Err(r) => return r,
    };
    let items = match st.servicio.store.buscar(&f) {
        Ok((v, _)) => v,
        Err(e) => return error_servicio(e.into()),
    };
    let mut out = String::from(
        "Fecha emisión,Tipo,Número,Identificación,Razón social,Clave de acceso,Nro. autorización,Estado,Valor total\r\n",
    );
    for r in items {
        let total = r.valor_total.map(|c| format!("{}.{:02}", c / 100, c % 100)).unwrap_or_default();
        let fila = [
            r.fecha_emision.clone().unwrap_or_default(),
            r.tipo_comprobante.clone(),
            numero_comprobante(&r.clave_acceso),
            r.identificacion.clone().unwrap_or_default(),
            r.razon_social.clone().unwrap_or_default(),
            r.clave_acceso.clone(),
            r.numero_autorizacion.clone().unwrap_or_default(),
            r.estado.clone(),
            total,
        ];
        out.push_str(&fila.iter().map(|c| celda(c)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"comprobantes.csv\""),
        ],
        out,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_neutraliza_formulas_y_comillas() {
        assert_eq!(celda("=1+1"), "\"'=1+1\"");
        assert_eq!(celda("Juan \"El\" Perez"), "\"Juan \"\"El\"\" Perez\"");
        assert_eq!(celda("normal"), "\"normal\"");
    }

    #[test]
    fn numero_y_fechas() {
        assert_eq!(numero_comprobante("2011202601140054458900110010010000000011234567819"), "001-001-000000001");
        assert_eq!(numero_comprobante("corta"), "");
        assert!(fecha_ok("2026-11-22"));
        assert!(!fecha_ok("22/11/2026") && !fecha_ok("2026-11-2x") && !fecha_ok("2026-1-22"));
    }
}
