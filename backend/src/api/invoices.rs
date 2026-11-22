//! Emisión, consulta y reintento de facturas.

use super::{
    errors::{error, service_error},
    AppState,
};
use crate::{
    invoice::{Buyer, Line, Vat},
    service::InvoiceRequest,
    store::Record,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct BuyerDto {
    pub id_type: String,
    pub business_name: String,
    pub id_number: String,
    pub address: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
pub enum VatDto {
    #[serde(rename = "15")]
    Fifteen,
    #[serde(rename = "5")]
    Five,
    #[serde(rename = "0")]
    Zero,
    #[serde(rename = "not_subject")]
    NotSubject,
    #[serde(rename = "exempt")]
    Exempt,
}

#[derive(Deserialize)]
pub struct LineDto {
    pub main_code: String,
    pub description: String,
    pub quantity: u32,
    pub unit_price_cents: i64,
    #[serde(default)]
    pub discount_cents: i64,
    pub vat: VatDto,
}

#[derive(Deserialize)]
pub struct InvoiceDto {
    /// Si falta, se emite a consumidor final.
    pub buyer: Option<BuyerDto>,
    pub lines: Vec<LineDto>,
    /// Tabla 24 del SRI; por defecto `01` (sin utilización del sistema financiero).
    pub payment_method: Option<String>,
    #[serde(default)]
    pub additional_info: Vec<(String, String)>,
}

#[derive(Serialize)]
pub struct RecordDto {
    pub access_key: String,
    /// `establishment-ptoEmi-sequence`, p. ej. `001-001-000000001`.
    pub number: String,
    pub sequence: u32,
    pub issue_date: Option<String>,
    pub document_type: String,
    pub id_number: Option<String>,
    pub business_name: Option<String>,
    /// Total en centavos.
    pub total_amount: Option<i64>,
    pub status: String,
    pub authorization_number: Option<String>,
    pub authorization_date: Option<String>,
    pub messages: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_xml: Option<String>,
}

impl RecordDto {
    /// Sin el XML firmado (para listados y respuestas de emisión).
    pub(super) fn summary(r: Record) -> Self {
        Self::build(r, false)
    }

    /// Con el XML firmado (consulta de un comprobante).
    pub(super) fn full(r: Record) -> Self {
        Self::build(r, true)
    }

    fn build(r: Record, with_xml: bool) -> Self {
        Self {
            number: document_number(&r.access_key),
            access_key: r.access_key,
            sequence: r.sequence,
            issue_date: r.issue_date,
            document_type: r.document_type,
            id_number: r.id_number,
            business_name: r.business_name,
            total_amount: r.total_amount,
            status: r.status,
            authorization_number: r.authorization_number,
            authorization_date: r.authorization_date,
            messages: r.messages,
            signed_xml: with_xml.then_some(r.signed_xml),
        }
    }
}

/// La serie y el secuencial van en las posiciones 24..39 de la clave de acceso.
pub(super) fn document_number(access_key: &str) -> String {
    if access_key.len() == 49 && access_key.is_ascii() {
        format!(
            "{}-{}-{}",
            &access_key[24..27],
            &access_key[27..30],
            &access_key[30..39]
        )
    } else {
        String::new()
    }
}

pub(super) async fn issue(State(st): State<Arc<AppState>>, Json(f): Json<InvoiceDto>) -> Response {
    let lines = f
        .lines
        .into_iter()
        .map(|l| Line {
            main_code: l.main_code,
            description: l.description,
            quantity: l.quantity,
            unit_price: l.unit_price_cents,
            discount: l.discount_cents,
            vat: match l.vat {
                VatDto::Fifteen => Vat::Fifteen,
                VatDto::Five => Vat::Five,
                VatDto::Zero => Vat::Zero,
                VatDto::NotSubject => Vat::NotSubject,
                VatDto::Exempt => Vat::Exempt,
            },
        })
        .collect();
    let buyer = f.buyer.map(|c| Buyer {
        id_type: c.id_type,
        business_name: c.business_name,
        id_number: c.id_number,
        address: c.address,
    });
    let request = InvoiceRequest {
        buyer: buyer.unwrap_or_else(Buyer::final_consumer),
        lines,
        payment_method: f.payment_method.unwrap_or_else(|| "01".into()),
        additional_info: f.additional_info,
    };
    match st.service.issue(request).await {
        Ok(r) => {
            let status = if r.status == "PENDIENTE" {
                StatusCode::ACCEPTED
            } else {
                StatusCode::CREATED
            };
            (status, Json(RecordDto::summary(r))).into_response()
        }
        Err(e) => service_error(e),
    }
}

pub(super) async fn get_invoice(
    State(st): State<Arc<AppState>>,
    Path(access_key): Path<String>,
) -> Response {
    match st.service.store.get_record(&access_key) {
        Ok(Some(r)) => Json(RecordDto::full(r)).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "comprobante no encontrado"),
        Err(e) => service_error(e.into()),
    }
}

pub(super) async fn retry(State(st): State<Arc<AppState>>) -> Response {
    match st.service.retry_pending().await {
        Ok(v) => {
            let out: Vec<_> = v
                .into_iter()
                .map(|(access_key, r)| match r {
                    Ok(record) => {
                        serde_json::json!({ "access_key": access_key, "status": record.status })
                    }
                    Err(e) => {
                        serde_json::json!({ "access_key": access_key, "error": e.to_string() })
                    }
                })
                .collect();
            Json(out).into_response()
        }
        Err(e) => service_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_number_comes_from_access_key() {
        assert_eq!(
            document_number("2011202601140054458900110010010000000011234567819"),
            "001-001-000000001"
        );
        assert_eq!(document_number("corta"), "");
    }
}
