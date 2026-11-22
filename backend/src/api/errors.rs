//! Respuestas de error JSON y su traducción desde los errores del dominio.

use crate::service::ServiceError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

pub(super) fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

pub(super) fn service_error(e: ServiceError) -> Response {
    match &e {
        ServiceError::InvalidData(_) => error(StatusCode::BAD_REQUEST, &e.to_string()),
        ServiceError::NotConfigured(_) => error(StatusCode::CONFLICT, &e.to_string()),
        ServiceError::Sri(_) => error(StatusCode::BAD_GATEWAY, &e.to_string()),
        // No se filtran detalles internos (rutas, SQL) al cliente.
        ServiceError::Store(_) | ServiceError::Signing(_) => {
            eprintln!("error interno: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "error interno")
        }
    }
}
