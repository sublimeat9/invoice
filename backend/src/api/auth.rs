//! Autenticación por token Bearer.

use super::{errors::error, AppState};
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // Comparación en tiempo constante respecto al contenido.
    let mut diff = (a.len() != b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

pub(super) async fn require_token(
    State(st): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| {
            !st.token.is_empty() && constant_time_eq(t.as_bytes(), st.token.as_bytes())
        });
    if ok {
        next.run(req).await
    } else {
        error(StatusCode::UNAUTHORIZED, "token inválido o ausente")
    }
}
