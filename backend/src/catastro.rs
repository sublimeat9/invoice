//! Consulta de RUC en el portal del SRI (SRI en línea). No es un servicio documentado en la
//! ficha técnica: son los mismos servicios REST que usa la página pública de consulta, así que
//! la lectura de la respuesta es tolerante y cualquier fallo se informa como "no disponible".

use crate::invoice::Issuer;
use serde_json::Value;
use std::time::Duration;

const DEFAULT_BASE: &str =
    "https://srienlinea.sri.gob.ec/sri-catastro-sujeto-servicio-internet/rest";

#[derive(Debug, thiserror::Error)]
pub enum CatastroError {
    #[error("el RUC debe tener 13 dígitos y terminar en 001 o similar")]
    BadShape,
    #[error("el RUC no existe en el SRI")]
    NotFound,
    #[error("el RUC no está activo en el SRI (estado: {0})")]
    Inactive(String),
    #[error("no se pudo consultar el SRI, intente más tarde: {0}")]
    Unavailable(String),
}

/// 13 dígitos y establecimiento distinto de 000: evita consultar con basura.
pub fn valid_shape(ruc: &str) -> bool {
    ruc.len() == 13 && ruc.bytes().all(|b| b.is_ascii_digit()) && !ruc.ends_with("000")
}

pub struct CatastroClient {
    http: reqwest::Client,
    base: String,
}

impl CatastroClient {
    pub fn new(base: Option<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("cliente HTTP"),
            base: base.unwrap_or_else(|| DEFAULT_BASE.into()),
        }
    }

    async fn get(&self, path: &str) -> Result<Value, CatastroError> {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .map_err(|e| CatastroError::Unavailable(e.to_string()))?;
        if !r.status().is_success() {
            return Err(CatastroError::Unavailable(format!("HTTP {}", r.status())));
        }
        let body = r
            .text()
            .await
            .map_err(|e| CatastroError::Unavailable(e.to_string()))?;
        serde_json::from_str(&body)
            .map_err(|e| CatastroError::Unavailable(format!("respuesta inesperada: {e}")))
    }

    /// Datos reales del contribuyente, listos para precargar el emisor.
    pub async fn lookup(&self, ruc: &str) -> Result<Issuer, CatastroError> {
        if !valid_shape(ruc) {
            return Err(CatastroError::BadShape);
        }
        let data = self
            .get(&format!(
                "/ConsolidadoContribuyente/obtenerPorNumerosRuc?&ruc={ruc}"
            ))
            .await?;
        let row = data
            .as_array()
            .and_then(|a| a.first())
            .or(Some(&data).filter(|d| d.is_object()))
            .ok_or(CatastroError::NotFound)?;
        let text = |v: &Value, k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let business_name = text(row, "razonSocial").ok_or(CatastroError::NotFound)?;
        let state = text(row, "estadoContribuyenteRuc").unwrap_or_default();
        if !state.eq_ignore_ascii_case("ACTIVO") {
            return Err(CatastroError::Inactive(state));
        }

        // La dirección matriz sale del establecimiento marcado como matriz.
        let establishments = self
            .get(&format!(
                "/Establecimiento/consultarPorNumeroRuc?numeroRuc={ruc}"
            ))
            .await
            .unwrap_or(Value::Null);
        let list = establishments.as_array().cloned().unwrap_or_default();
        let head = list
            .iter()
            .find(|e| text(e, "matriz").is_some_and(|m| m.eq_ignore_ascii_case("SI")))
            .or(list.first());
        Ok(Issuer {
            ruc: ruc.to_string(),
            business_name,
            trade_name: text(row, "nombreComercial")
                .or_else(|| head.and_then(|h| text(h, "nombreFantasiaComercial"))),
            head_office_address: head
                .and_then(|h| text(h, "direccionCompleta"))
                .unwrap_or_default(),
            branch_address: None,
            accounting_required: text(row, "obligadoLlevarContabilidad")
                .is_some_and(|v| v.eq_ignore_ascii_case("SI")),
            establishment: "001".into(),
            emission_point: "001".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruc_shape() {
        assert!(valid_shape("1760013210001"));
        assert!(!valid_shape("1760013210000"));
        assert!(!valid_shape("176001321"));
        assert!(!valid_shape("17600132100a1"));
    }
}
