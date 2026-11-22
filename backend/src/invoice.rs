//! XML de factura versión 1.1.0 (ficha técnica SRI v2.34, anexo 3).
//!
//! Los montos se manejan en centavos (`i64`) para evitar errores de punto flotante.
//! El XML se emite ya en forma canónica (sin declaración de namespaces, atributos
//! ordenados, sin etiquetas vacías autocerradas) para facilitar la firma.

use crate::access_key::{self, AccessKeyParams, DocumentType, Environment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vat {
    Zero,
    Fifteen,
    Five,
    NotSubject,
    Exempt,
}

impl Vat {
    pub fn percentage_code(self) -> &'static str {
        match self {
            Self::Zero => "0",
            Self::Fifteen => "4",
            Self::Five => "5",
            Self::NotSubject => "6",
            Self::Exempt => "7",
        }
    }
    /// Tarifa en puntos porcentuales.
    pub fn rate(self) -> i64 {
        match self {
            Self::Fifteen => 15,
            Self::Five => 5,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Issuer {
    pub ruc: String,
    pub business_name: String,
    pub trade_name: Option<String>,
    pub head_office_address: String,
    pub branch_address: Option<String>,
    #[serde(default)]
    pub accounting_required: bool,
    /// 3 dígitos, p. ej. `001`.
    pub establishment: String,
    pub emission_point: String,
}

impl Issuer {
    /// Carga el emisor desde variables de entorno:
    /// `EMISOR_RUC`, `EMISOR_RAZON_SOCIAL`, `EMISOR_DIR_MATRIZ` (obligatorias);
    /// `EMISOR_NOMBRE_COMERCIAL`, `EMISOR_DIR_ESTABLECIMIENTO` (opcionales);
    /// `EMISOR_OBLIGADO_CONTABILIDAD` (`SI`/`NO`, por defecto `NO`);
    /// `EMISOR_ESTAB` y `EMISOR_PTO_EMI` (3 dígitos, por defecto `001`).
    pub fn from_env() -> Result<Self, String> {
        let req = |k: &str| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| format!("falta la variable de entorno {k}"))
        };
        let opt = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let e = Self {
            ruc: req("EMISOR_RUC")?,
            business_name: req("EMISOR_RAZON_SOCIAL")?,
            trade_name: opt("EMISOR_NOMBRE_COMERCIAL"),
            head_office_address: req("EMISOR_DIR_MATRIZ")?,
            branch_address: opt("EMISOR_DIR_ESTABLECIMIENTO"),
            accounting_required: opt("EMISOR_OBLIGADO_CONTABILIDAD")
                .map(|v| v.eq_ignore_ascii_case("SI"))
                .unwrap_or(false),
            establishment: opt("EMISOR_ESTAB").unwrap_or_else(|| "001".into()),
            emission_point: opt("EMISOR_PTO_EMI").unwrap_or_else(|| "001".into()),
        };
        e.validate()?;
        Ok(e)
    }

    pub fn validate(&self) -> Result<(), String> {
        let num = |s: &str, n: usize, name: &str| {
            if s.len() == n && s.bytes().all(|b| b.is_ascii_digit()) {
                Ok(())
            } else {
                Err(format!("{name} debe tener {n} dígitos: {s:?}"))
            }
        };
        num(&self.ruc, 13, "EMISOR_RUC")?;
        num(&self.establishment, 3, "EMISOR_ESTAB")?;
        num(&self.emission_point, 3, "EMISOR_PTO_EMI")?;
        if self.business_name.len() > 300 || self.head_office_address.len() > 300 {
            return Err("razón social y dirección matriz admiten máximo 300 caracteres".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Buyer {
    /// Tabla 6: 04 RUC, 05 cédula, 06 pasaporte, 07 consumidor final, 08 exterior.
    pub id_type: String,
    pub business_name: String,
    pub id_number: String,
    pub address: Option<String>,
}

impl Buyer {
    pub fn final_consumer() -> Self {
        Self {
            id_type: "07".into(),
            business_name: "CONSUMIDOR FINAL".into(),
            id_number: "9999999999999".into(),
            address: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Line {
    pub main_code: String,
    pub description: String,
    pub quantity: u32,
    /// Precio unitario en centavos.
    pub unit_price: i64,
    /// Descuento total de la línea en centavos.
    pub discount: i64,
    pub vat: Vat,
}

impl Line {
    fn base(&self) -> i64 {
        self.quantity as i64 * self.unit_price - self.discount
    }
    fn vat_amount(&self) -> i64 {
        // Redondeo half-up a centavos.
        (self.base() * self.vat.rate() + 50) / 100
    }
}

#[derive(Debug, Clone)]
pub struct Invoice {
    pub environment: Environment,
    pub issuer: Issuer,
    pub buyer: Buyer,
    /// `ddmmaaaa`.
    pub issue_date: String,
    pub sequence: u32,
    /// 8 dígitos.
    pub numeric_code: String,
    pub lines: Vec<Line>,
    /// Tabla 24 (p. ej. `01` sin sistema financiero, `20` otros con sistema financiero).
    pub payment_method: String,
    /// Pares (nombre, valor) de `infoAdicional`, p. ej. ("Email", "a@b.com"). Máximo 15.
    pub additional_info: Vec<(String, String)>,
}

pub fn cents(v: i64) -> String {
    format!(
        "{}{}.{:02}",
        if v < 0 { "-" } else { "" },
        v.abs() / 100,
        v.abs() % 100
    )
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;")
}

fn tag(out: &mut String, name: &str, val: &str) {
    out.push_str(&format!("<{name}>{}</{name}>", esc(val)));
}

impl Invoice {
    /// Total a pagar en centavos (subtotal + IVA).
    pub fn grand_total(&self) -> i64 {
        self.lines.iter().map(|l| l.base() + l.vat_amount()).sum()
    }

    /// Fecha de emisión como `aaaa-mm-dd` (ordenable en SQL).
    pub fn iso_date(&self) -> String {
        let f = &self.issue_date;
        format!("{}-{}-{}", &f[4..8], &f[2..4], &f[0..2])
    }

    pub fn access_key(&self) -> Result<String, String> {
        access_key::generate(&AccessKeyParams {
            issue_date: &self.issue_date,
            kind: DocumentType::Invoice,
            ruc: &self.issuer.ruc,
            environment: self.environment,
            series: &format!(
                "{}{}",
                self.issuer.establishment, self.issuer.emission_point
            ),
            sequence: self.sequence,
            numeric_code: &self.numeric_code,
        })
    }

    /// Devuelve `(xml, access_key)`. El XML no lleva declaración `<?xml?>` para que
    /// el digest coincida byte a byte con el documento firmado; se agrega al firmar.
    pub fn to_xml(&self) -> Result<(String, String), String> {
        if self.lines.is_empty() {
            return Err("la factura necesita al menos una línea".into());
        }
        if self.additional_info.len() > 15 {
            return Err("infoAdicional admite máximo 15 campos".into());
        }
        for (n, v) in &self.additional_info {
            if n.trim().is_empty() || v.trim().is_empty() || v.len() > 300 || n.len() > 300 {
                return Err(format!("campoAdicional inválido: {n:?}"));
            }
        }
        let access_key = self.access_key()?;
        let e = &self.issuer;
        let f = &self.issue_date;
        let date = format!("{}/{}/{}", &f[0..2], &f[2..4], &f[4..8]);

        let total_without_tax: i64 = self.lines.iter().map(Line::base).sum();
        let total_discount: i64 = self.lines.iter().map(|l| l.discount).sum();
        // Agrupa por código de porcentaje, conservando orden de aparición.
        let mut groups: Vec<(Vat, i64, i64)> = vec![];
        for l in &self.lines {
            match groups.iter_mut().find(|g| g.0 == l.vat) {
                Some(g) => {
                    g.1 += l.base();
                    g.2 += l.vat_amount();
                }
                None => groups.push((l.vat, l.base(), l.vat_amount())),
            }
        }
        let total_vat: i64 = groups.iter().map(|g| g.2).sum();
        let grand_total = total_without_tax + total_vat;

        let mut x = String::new();
        x.push_str(r#"<factura id="comprobante" version="1.1.0">"#);
        x.push_str("<infoTributaria>");
        tag(&mut x, "ambiente", &self.environment.code().to_string());
        tag(&mut x, "tipoEmision", "1");
        tag(&mut x, "razonSocial", &e.business_name);
        if let Some(n) = &e.trade_name {
            tag(&mut x, "nombreComercial", n);
        }
        tag(&mut x, "ruc", &e.ruc);
        tag(&mut x, "claveAcceso", &access_key);
        tag(&mut x, "codDoc", "01");
        tag(&mut x, "estab", &e.establishment);
        tag(&mut x, "ptoEmi", &e.emission_point);
        tag(&mut x, "secuencial", &format!("{:09}", self.sequence));
        tag(&mut x, "dirMatriz", &e.head_office_address);
        x.push_str("</infoTributaria><infoFactura>");
        tag(&mut x, "fechaEmision", &date);
        if let Some(d) = &e.branch_address {
            tag(&mut x, "dirEstablecimiento", d);
        }
        tag(
            &mut x,
            "obligadoContabilidad",
            if e.accounting_required { "SI" } else { "NO" },
        );
        let c = &self.buyer;
        tag(&mut x, "tipoIdentificacionComprador", &c.id_type);
        tag(&mut x, "razonSocialComprador", &c.business_name);
        tag(&mut x, "identificacionComprador", &c.id_number);
        if let Some(d) = &c.address {
            tag(&mut x, "direccionComprador", d);
        }
        tag(&mut x, "totalSinImpuestos", &cents(total_without_tax));
        tag(&mut x, "totalDescuento", &cents(total_discount));
        x.push_str("<totalConImpuestos>");
        for (vat, base, value) in &groups {
            x.push_str("<totalImpuesto>");
            tag(&mut x, "codigo", "2");
            tag(&mut x, "codigoPorcentaje", vat.percentage_code());
            tag(&mut x, "baseImponible", &cents(*base));
            tag(&mut x, "valor", &cents(*value));
            x.push_str("</totalImpuesto>");
        }
        x.push_str("</totalConImpuestos>");
        tag(&mut x, "propina", "0.00");
        tag(&mut x, "importeTotal", &cents(grand_total));
        tag(&mut x, "moneda", "DOLAR");
        x.push_str("<pagos><pago>");
        tag(&mut x, "formaPago", &self.payment_method);
        tag(&mut x, "total", &cents(grand_total));
        x.push_str("</pago></pagos></infoFactura><detalles>");
        for l in &self.lines {
            x.push_str("<detalle>");
            tag(&mut x, "codigoPrincipal", &l.main_code);
            tag(&mut x, "descripcion", &l.description);
            tag(&mut x, "cantidad", &l.quantity.to_string());
            tag(&mut x, "precioUnitario", &cents(l.unit_price));
            tag(&mut x, "descuento", &cents(l.discount));
            tag(&mut x, "precioTotalSinImpuesto", &cents(l.base()));
            x.push_str("<impuestos><impuesto>");
            tag(&mut x, "codigo", "2");
            tag(&mut x, "codigoPorcentaje", l.vat.percentage_code());
            tag(&mut x, "tarifa", &format!("{}.00", l.vat.rate()));
            tag(&mut x, "baseImponible", &cents(l.base()));
            tag(&mut x, "valor", &cents(l.vat_amount()));
            x.push_str("</impuesto></impuestos></detalle>");
        }
        x.push_str("</detalles>");
        if !self.additional_info.is_empty() {
            x.push_str("<infoAdicional>");
            for (n, v) in &self.additional_info {
                x.push_str(&format!(
                    r#"<campoAdicional nombre="{}">{}</campoAdicional>"#,
                    esc_attr(n),
                    esc(v)
                ));
            }
            x.push_str("</infoAdicional>");
        }
        x.push_str("</factura>");
        Ok((x, access_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn example() -> Invoice {
        Invoice {
            environment: Environment::Testing,
            issuer: Issuer {
                ruc: "1760013210001".into(),
                business_name: "PRUEBAS & CIA".into(),
                trade_name: None,
                head_office_address: "QUITO".into(),
                branch_address: None,
                accounting_required: false,
                establishment: "001".into(),
                emission_point: "001".into(),
            },
            buyer: Buyer::final_consumer(),
            issue_date: "20112026".into(),
            sequence: 1,
            numeric_code: "12345678".into(),
            lines: vec![Line {
                main_code: "S/C".into(),
                description: "ASISTENCIA".into(),
                quantity: 1,
                unit_price: 1500,
                discount: 0,
                vat: Vat::Fifteen,
            }],
            payment_method: "01".into(),
            additional_info: vec![],
        }
    }

    #[test]
    fn additional_info_is_escaped_and_last() {
        let mut f = example();
        f.additional_info = vec![
            ("Dirección \"A\"".into(), "Calle 1 & 2".into()),
            ("Email".into(), "a@b.com".into()),
        ];
        let (xml, _) = f.to_xml().unwrap();
        assert!(xml.ends_with(
            r#"<campoAdicional nombre="Email">a@b.com</campoAdicional></infoAdicional></factura>"#
        ));
        assert!(xml.contains(r#"nombre="Dirección &quot;A&quot;">Calle 1 &amp; 2"#));
        roxmltree::Document::parse(&xml).unwrap();
        f.additional_info = (0..16).map(|i| (format!("c{i}"), "v".into())).collect();
        assert!(f.to_xml().is_err());
    }

    #[test]
    fn totals_vat_15() {
        let (xml, access_key) = example().to_xml().unwrap();
        assert!(access_key::validate(&access_key));
        assert!(xml.contains("<importeTotal>17.25</importeTotal>"));
        assert_eq!(example().grand_total(), 1725);
        assert_eq!(example().iso_date(), "2026-11-20");
        assert!(xml.contains("<valor>2.25</valor>"));
        assert!(xml.contains("PRUEBAS &amp; CIA"));
        roxmltree::Document::parse(&xml).unwrap();
    }
}

#[cfg(test)]
mod issuer_tests {
    use super::*;

    #[test]
    fn validate_issuer() {
        let mut e = Issuer {
            ruc: "1400544589001".into(),
            business_name: "X".into(),
            trade_name: None,
            head_office_address: "Y".into(),
            branch_address: None,
            accounting_required: false,
            establishment: "001".into(),
            emission_point: "001".into(),
        };
        assert!(e.validate().is_ok());
        e.ruc = "123".into();
        assert!(e.validate().is_err());
    }
}
