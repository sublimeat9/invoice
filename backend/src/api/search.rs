//! Búsqueda de comprobantes con filtros y reporte CSV.

use super::{
    errors::{error, service_error},
    invoices::{document_number, RecordDto},
    AppState,
};
use crate::accounts::User;
use crate::store::Filter;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize, Default)]
pub struct SearchQuery {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub id_number: Option<String>,
    pub document_type: Option<String>,
    pub status: Option<String>,
    pub access_key: Option<String>,
    pub limit: Option<u32>,
    /// Empieza en 1.
    pub page: Option<u32>,
}

pub(super) fn is_valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

pub(super) fn filter(q: SearchQuery) -> Result<Filter, String> {
    let empty = |o: Option<String>| o.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let date_from = empty(q.date_from);
    let date_to = empty(q.date_to);
    for f in [&date_from, &date_to].into_iter().flatten() {
        if !is_valid_date(f) {
            return Err("las fechas deben tener formato aaaa-mm-dd".into());
        }
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 10_000);
    Ok(Filter {
        date_from,
        date_to,
        id_number: empty(q.id_number),
        document_type: empty(q.document_type),
        status: empty(q.status),
        access_key: empty(q.access_key),
        limit: Some(limit),
        offset: Some(
            q.page
                .unwrap_or(1)
                .max(1)
                .saturating_sub(1)
                .saturating_mul(limit),
        ),
    })
}

pub(super) async fn search(
    State(st): State<Arc<AppState>>,
    Extension(me): Extension<User>,
    Query(q): Query<SearchQuery>,
) -> Response {
    let f = match filter(q) {
        Ok(f) => f,
        Err(m) => return error(StatusCode::BAD_REQUEST, &m),
    };
    match st.service.store.search(me.id, &f).await {
        Ok((v, total)) => Json(serde_json::json!({
            "total": total,
            "items": v.into_iter().map(RecordDto::summary).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => service_error(e.into()),
    }
}

/// Celda CSV: se entrecomilla y se neutralizan fórmulas (`=`, `+`, `-`, `@`) para que
/// abrir el reporte en una hoja de cálculo no ejecute contenido del comprador.
pub(super) fn cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub(super) async fn report(
    State(st): State<Arc<AppState>>,
    Extension(me): Extension<User>,
    Query(mut q): Query<SearchQuery>,
) -> Response {
    q.limit = Some(10_000);
    q.page = Some(1);
    let f = match filter(q) {
        Ok(f) => f,
        Err(m) => return error(StatusCode::BAD_REQUEST, &m),
    };
    let items = match st.service.store.search(me.id, &f).await {
        Ok((v, _)) => v,
        Err(e) => return service_error(e.into()),
    };
    let mut out = String::from(
        "Fecha emisión,Tipo,Número,Identificación,Razón social,Clave de acceso,Nro. autorización,Estado,Valor total\r\n",
    );
    for r in items {
        let total = r
            .total_amount
            .map(|c| format!("{}.{:02}", c / 100, c % 100))
            .unwrap_or_default();
        let row_to_record = [
            r.issue_date.clone().unwrap_or_default(),
            r.document_type.clone(),
            document_number(&r.access_key),
            r.id_number.clone().unwrap_or_default(),
            r.business_name.clone().unwrap_or_default(),
            r.access_key.clone(),
            r.authorization_number.clone().unwrap_or_default(),
            r.status.clone(),
            total,
        ];
        out.push_str(
            &row_to_record
                .iter()
                .map(|c| cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"comprobantes.csv\"",
            ),
        ],
        out,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_neutralizes_formulas_and_quotes() {
        assert_eq!(cell("=1+1"), "\"'=1+1\"");
        assert_eq!(cell("Juan \"El\" Perez"), "\"Juan \"\"El\"\" Perez\"");
        assert_eq!(cell("normal"), "\"normal\"");
    }

    #[test]
    fn dates_use_yyyy_mm_dd_format() {
        assert!(is_valid_date("2026-11-22"));
        assert!(
            !is_valid_date("22/11/2026")
                && !is_valid_date("2026-11-2x")
                && !is_valid_date("2026-1-22")
        );
    }
}
