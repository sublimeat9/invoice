//! API HTTP (JSON) sobre `Servicio`.
//!
//! - `GET  /health`                 sin autenticación
//! - `GET  /facturas`               últimos 50 comprobantes (sin XML)
//! - `POST /facturas`               emite una factura (201 si hay resultado final, 202 si quedó PENDIENTE)
//! - `GET  /facturas/{clave}`       estado guardado + XML firmado
//! - `POST /facturas/reintentar`    reenvía las PENDIENTE
//!
//! Autenticación: `Authorization: Bearer <API_TOKEN>`. Los montos van en centavos enteros.

use crate::{
    factura::{Comprador, Iva, Linea},
    servicio::{Servicio, ServicioError, SolicitudFactura},
    store::Registro,
};
use axum::{
    extract::{Path, Request, State},
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
        .route("/facturas", post(emitir).get(listar))
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
    pub secuencial: u32,
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
            clave_acceso: r.clave_acceso,
            secuencial: r.secuencial,
            estado: r.estado,
            numero_autorizacion: r.numero_autorizacion,
            fecha_autorizacion: r.fecha_autorizacion,
            mensajes: r.mensajes,
            xml_firmado: con_xml.then_some(r.xml_firmado),
        }
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

async fn listar(State(st): State<Arc<AppState>>) -> Response {
    match st.servicio.store.listar(50) {
        Ok(v) => Json(v.into_iter().map(|r| RegistroDto::de(r, false)).collect::<Vec<_>>()).into_response(),
        Err(e) => error_servicio(e.into()),
    }
}
