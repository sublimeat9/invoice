//! Configuración: emisor, ambiente y certificado.

use super::{
    errors::{error, service_error},
    AppState,
};
use crate::accounts::User;
use crate::{
    access_key::Environment, invoice::Issuer, settings::parse_environment, signing::CertSigner,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;

pub(super) async fn view_config(
    State(st): State<Arc<AppState>>,
    Extension(me): Extension<User>,
) -> Response {
    let sv = &st.service;
    let issuer = match sv.store.load_issuer(me.id) {
        Ok(e) => e,
        Err(e) => return service_error(e.into()),
    };
    let certificate = match sv.store.load_certificate(me.id, sv.encryption_key()) {
        Ok(None) => serde_json::json!({ "loaded": false }),
        Ok(Some((p12, pass))) => match CertSigner::from_p12(&p12, &pass).and_then(|f| f.info()) {
            Ok(i) => {
                serde_json::json!({ "loaded": true, "subject": i.subject, "expires": i.expires, "valid": i.valid })
            }
            Err(e) => serde_json::json!({ "loaded": true, "error": e.to_string() }),
        },
        Err(e) => serde_json::json!({ "loaded": true, "error": e }),
    };
    let environment = issuer
        .as_ref()
        .map(|(_, a)| *a)
        .unwrap_or(Environment::Testing);
    Json(serde_json::json!({
        "issuer": issuer.map(|(e, _)| e),
        "environment": if environment == Environment::Production { "production" } else { "test" },
        "certificate": certificate,
        "ready": sv.config(me.id).is_ok(),
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct IssuerConfigDto {
    #[serde(flatten)]
    pub issuer: Issuer,
    pub environment: String,
}

pub(super) async fn save_issuer(
    State(st): State<Arc<AppState>>,
    Extension(me): Extension<User>,
    Json(c): Json<IssuerConfigDto>,
) -> Response {
    let mut e = c.issuer;
    // Se recortan espacios; los opcionales vacíos pasan a `None`.
    e.ruc = e.ruc.trim().into();
    e.business_name = e.business_name.trim().into();
    e.head_office_address = e.head_office_address.trim().into();
    e.establishment = e.establishment.trim().into();
    e.emission_point = e.emission_point.trim().into();
    e.trade_name = e
        .trade_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    e.branch_address = e
        .branch_address
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if e.business_name.is_empty() || e.head_office_address.is_empty() {
        return error(
            StatusCode::BAD_REQUEST,
            "razón social y dirección matriz son obligatorias",
        );
    }
    if let Err(m) = e.validate() {
        return error(StatusCode::BAD_REQUEST, &m);
    }
    let Some(env_label) = parse_environment(&c.environment) else {
        return error(
            StatusCode::BAD_REQUEST,
            "ambiente debe ser pruebas o produccion",
        );
    };
    match st.service.store.save_issuer(me.id, &e, env_label) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => service_error(e.into()),
    }
}

#[derive(Deserialize)]
pub struct CertificateDto {
    pub p12_base64: String,
    pub password: String,
}

pub(super) async fn save_certificate(
    State(st): State<Arc<AppState>>,
    Extension(me): Extension<User>,
    Json(c): Json<CertificateDto>,
) -> Response {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let Ok(p12) = STANDARD.decode(c.p12_base64.trim()) else {
        return error(StatusCode::BAD_REQUEST, "el archivo no es base64 válido");
    };
    if p12.is_empty() || p12.len() > 1_000_000 {
        return error(
            StatusCode::BAD_REQUEST,
            "archivo .p12 vacío o demasiado grande",
        );
    }
    // Se valida antes de guardar: contraseña correcta, clave RSA y certificado vigente.
    let info = match CertSigner::from_p12(&p12, &c.password).and_then(|f| f.info()) {
        Ok(i) => i,
        Err(e) => return error(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    if !info.valid {
        return error(
            StatusCode::BAD_REQUEST,
            &format!("el certificado está vencido (venció el {})", info.expires),
        );
    }
    match st
        .service
        .store
        .save_certificate(me.id, st.service.encryption_key(), &p12, &c.password)
    {
        Ok(()) => Json(info).into_response(),
        Err(e) => service_error(e.into()),
    }
}
