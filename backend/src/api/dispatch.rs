//! Invocación de la API dentro del proceso, sin red.

use axum::{extract::Request, http::header, Router};

/// Invoca la API **dentro del proceso** (sin red): lo usa la app de escritorio, donde la
/// interfaz y el backend viven juntos. Devuelve `(código HTTP, body)`.
pub async fn dispatch(
    app: Router,
    token: &str,
    method: &str,
    path: &str,
    body: Option<String>,
) -> Result<(u16, String), String> {
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    if !path.starts_with('/') || path.starts_with("//") {
        return Err("ruta inválida".into());
    }
    let method: axum::http::Method = method.parse().map_err(|_| "método inválido".to_string())?;
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    if body.is_some() {
        req = req.header(header::CONTENT_TYPE, "application/json");
    }
    let req = req
        .body(axum::body::Body::from(body.unwrap_or_default()))
        .map_err(|e| e.to_string())?;
    // El error del router es `Infallible`: este patrón no puede fallar.
    let Ok(resp) = app.oneshot(req).await;
    let status = resp.status().as_u16();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| e.to_string())?
        .to_bytes();
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}
