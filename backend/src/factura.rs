//! XML de factura versión 1.1.0 (ficha técnica SRI v2.34, anexo 3).
//!
//! Los montos se manejan en centavos (`i64`) para evitar errores de punto flotante.
//! El XML se emite ya en forma canónica (sin declaración de namespaces, atributos
//! ordenados, sin etiquetas vacías autocerradas) para facilitar la firma.

use crate::clave_acceso::{self, Ambiente, ClaveAccesoParams, TipoComprobante};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Iva {
    Cero,
    Quince,
    Cinco,
    NoObjeto,
    Exento,
}

impl Iva {
    pub fn codigo_porcentaje(self) -> &'static str {
        match self {
            Self::Cero => "0",
            Self::Quince => "4",
            Self::Cinco => "5",
            Self::NoObjeto => "6",
            Self::Exento => "7",
        }
    }
    /// Tarifa en puntos porcentuales.
    pub fn tarifa(self) -> i64 {
        match self {
            Self::Quince => 15,
            Self::Cinco => 5,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Emisor {
    pub ruc: String,
    pub razon_social: String,
    pub nombre_comercial: Option<String>,
    pub dir_matriz: String,
    pub dir_establecimiento: Option<String>,
    pub obligado_contabilidad: bool,
    /// 3 dígitos, p. ej. `001`.
    pub estab: String,
    pub pto_emi: String,
}

impl Emisor {
    /// Carga el emisor desde variables de entorno:
    /// `EMISOR_RUC`, `EMISOR_RAZON_SOCIAL`, `EMISOR_DIR_MATRIZ` (obligatorias);
    /// `EMISOR_NOMBRE_COMERCIAL`, `EMISOR_DIR_ESTABLECIMIENTO` (opcionales);
    /// `EMISOR_OBLIGADO_CONTABILIDAD` (`SI`/`NO`, por defecto `NO`);
    /// `EMISOR_ESTAB` y `EMISOR_PTO_EMI` (3 dígitos, por defecto `001`).
    pub fn desde_env() -> Result<Self, String> {
        let req = |k: &str| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| format!("falta la variable de entorno {k}"))
        };
        let opt = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let e = Self {
            ruc: req("EMISOR_RUC")?,
            razon_social: req("EMISOR_RAZON_SOCIAL")?,
            nombre_comercial: opt("EMISOR_NOMBRE_COMERCIAL"),
            dir_matriz: req("EMISOR_DIR_MATRIZ")?,
            dir_establecimiento: opt("EMISOR_DIR_ESTABLECIMIENTO"),
            obligado_contabilidad: opt("EMISOR_OBLIGADO_CONTABILIDAD")
                .map(|v| v.eq_ignore_ascii_case("SI"))
                .unwrap_or(false),
            estab: opt("EMISOR_ESTAB").unwrap_or_else(|| "001".into()),
            pto_emi: opt("EMISOR_PTO_EMI").unwrap_or_else(|| "001".into()),
        };
        e.validar()?;
        Ok(e)
    }

    pub fn validar(&self) -> Result<(), String> {
        let num = |s: &str, n: usize, nombre: &str| {
            if s.len() == n && s.bytes().all(|b| b.is_ascii_digit()) {
                Ok(())
            } else {
                Err(format!("{nombre} debe tener {n} dígitos: {s:?}"))
            }
        };
        num(&self.ruc, 13, "EMISOR_RUC")?;
        num(&self.estab, 3, "EMISOR_ESTAB")?;
        num(&self.pto_emi, 3, "EMISOR_PTO_EMI")?;
        if self.razon_social.len() > 300 || self.dir_matriz.len() > 300 {
            return Err("razón social y dirección matriz admiten máximo 300 caracteres".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Comprador {
    /// Tabla 6: 04 RUC, 05 cédula, 06 pasaporte, 07 consumidor final, 08 exterior.
    pub tipo_identificacion: String,
    pub razon_social: String,
    pub identificacion: String,
    pub direccion: Option<String>,
}

impl Comprador {
    pub fn consumidor_final() -> Self {
        Self {
            tipo_identificacion: "07".into(),
            razon_social: "CONSUMIDOR FINAL".into(),
            identificacion: "9999999999999".into(),
            direccion: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Linea {
    pub codigo_principal: String,
    pub descripcion: String,
    pub cantidad: u32,
    /// Precio unitario en centavos.
    pub precio_unitario: i64,
    /// Descuento total de la línea en centavos.
    pub descuento: i64,
    pub iva: Iva,
}

impl Linea {
    fn base(&self) -> i64 {
        self.cantidad as i64 * self.precio_unitario - self.descuento
    }
    fn iva_valor(&self) -> i64 {
        // Redondeo half-up a centavos.
        (self.base() * self.iva.tarifa() + 50) / 100
    }
}

#[derive(Debug, Clone)]
pub struct Factura {
    pub ambiente: Ambiente,
    pub emisor: Emisor,
    pub comprador: Comprador,
    /// `ddmmaaaa`.
    pub fecha_emision: String,
    pub secuencial: u32,
    /// 8 dígitos.
    pub codigo_numerico: String,
    pub lineas: Vec<Linea>,
    /// Tabla 24 (p. ej. `01` sin sistema financiero, `20` otros con sistema financiero).
    pub forma_pago: String,
    /// Pares (nombre, valor) de `infoAdicional`, p. ej. ("Email", "a@b.com"). Máximo 15.
    pub info_adicional: Vec<(String, String)>,
}

pub fn cents(v: i64) -> String {
    format!("{}{}.{:02}", if v < 0 { "-" } else { "" }, v.abs() / 100, v.abs() % 100)
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;")
}

fn tag(out: &mut String, name: &str, val: &str) {
    out.push_str(&format!("<{name}>{}</{name}>", esc(val)));
}

impl Factura {
    /// Total a pagar en centavos (subtotal + IVA).
    pub fn importe_total(&self) -> i64 {
        self.lineas.iter().map(|l| l.base() + l.iva_valor()).sum()
    }

    /// Fecha de emisión como `aaaa-mm-dd` (ordenable en SQL).
    pub fn fecha_iso(&self) -> String {
        let f = &self.fecha_emision;
        format!("{}-{}-{}", &f[4..8], &f[2..4], &f[0..2])
    }

    pub fn clave_acceso(&self) -> Result<String, String> {
        clave_acceso::generar(&ClaveAccesoParams {
            fecha_emision: &self.fecha_emision,
            tipo: TipoComprobante::Factura,
            ruc: &self.emisor.ruc,
            ambiente: self.ambiente,
            serie: &format!("{}{}", self.emisor.estab, self.emisor.pto_emi),
            secuencial: self.secuencial,
            codigo_numerico: &self.codigo_numerico,
        })
    }

    /// Devuelve `(xml, clave_acceso)`. El XML no lleva declaración `<?xml?>` para que
    /// el digest coincida byte a byte con el documento firmado; se agrega al firmar.
    pub fn to_xml(&self) -> Result<(String, String), String> {
        if self.lineas.is_empty() {
            return Err("la factura necesita al menos una línea".into());
        }
        if self.info_adicional.len() > 15 {
            return Err("infoAdicional admite máximo 15 campos".into());
        }
        for (n, v) in &self.info_adicional {
            if n.trim().is_empty() || v.trim().is_empty() || v.len() > 300 || n.len() > 300 {
                return Err(format!("campoAdicional inválido: {n:?}"));
            }
        }
        let clave = self.clave_acceso()?;
        let e = &self.emisor;
        let f = &self.fecha_emision;
        let fecha = format!("{}/{}/{}", &f[0..2], &f[2..4], &f[4..8]);

        let total_sin_imp: i64 = self.lineas.iter().map(Linea::base).sum();
        let total_desc: i64 = self.lineas.iter().map(|l| l.descuento).sum();
        // Agrupa por código de porcentaje, conservando orden de aparición.
        let mut grupos: Vec<(Iva, i64, i64)> = vec![];
        for l in &self.lineas {
            match grupos.iter_mut().find(|g| g.0 == l.iva) {
                Some(g) => {
                    g.1 += l.base();
                    g.2 += l.iva_valor();
                }
                None => grupos.push((l.iva, l.base(), l.iva_valor())),
            }
        }
        let total_iva: i64 = grupos.iter().map(|g| g.2).sum();
        let importe_total = total_sin_imp + total_iva;

        let mut x = String::new();
        x.push_str(r#"<factura id="comprobante" version="1.1.0">"#);
        x.push_str("<infoTributaria>");
        tag(&mut x, "ambiente", &self.ambiente.codigo().to_string());
        tag(&mut x, "tipoEmision", "1");
        tag(&mut x, "razonSocial", &e.razon_social);
        if let Some(n) = &e.nombre_comercial {
            tag(&mut x, "nombreComercial", n);
        }
        tag(&mut x, "ruc", &e.ruc);
        tag(&mut x, "claveAcceso", &clave);
        tag(&mut x, "codDoc", "01");
        tag(&mut x, "estab", &e.estab);
        tag(&mut x, "ptoEmi", &e.pto_emi);
        tag(&mut x, "secuencial", &format!("{:09}", self.secuencial));
        tag(&mut x, "dirMatriz", &e.dir_matriz);
        x.push_str("</infoTributaria><infoFactura>");
        tag(&mut x, "fechaEmision", &fecha);
        if let Some(d) = &e.dir_establecimiento {
            tag(&mut x, "dirEstablecimiento", d);
        }
        tag(&mut x, "obligadoContabilidad", if e.obligado_contabilidad { "SI" } else { "NO" });
        let c = &self.comprador;
        tag(&mut x, "tipoIdentificacionComprador", &c.tipo_identificacion);
        tag(&mut x, "razonSocialComprador", &c.razon_social);
        tag(&mut x, "identificacionComprador", &c.identificacion);
        if let Some(d) = &c.direccion {
            tag(&mut x, "direccionComprador", d);
        }
        tag(&mut x, "totalSinImpuestos", &cents(total_sin_imp));
        tag(&mut x, "totalDescuento", &cents(total_desc));
        x.push_str("<totalConImpuestos>");
        for (iva, base, valor) in &grupos {
            x.push_str("<totalImpuesto>");
            tag(&mut x, "codigo", "2");
            tag(&mut x, "codigoPorcentaje", iva.codigo_porcentaje());
            tag(&mut x, "baseImponible", &cents(*base));
            tag(&mut x, "valor", &cents(*valor));
            x.push_str("</totalImpuesto>");
        }
        x.push_str("</totalConImpuestos>");
        tag(&mut x, "propina", "0.00");
        tag(&mut x, "importeTotal", &cents(importe_total));
        tag(&mut x, "moneda", "DOLAR");
        x.push_str("<pagos><pago>");
        tag(&mut x, "formaPago", &self.forma_pago);
        tag(&mut x, "total", &cents(importe_total));
        x.push_str("</pago></pagos></infoFactura><detalles>");
        for l in &self.lineas {
            x.push_str("<detalle>");
            tag(&mut x, "codigoPrincipal", &l.codigo_principal);
            tag(&mut x, "descripcion", &l.descripcion);
            tag(&mut x, "cantidad", &l.cantidad.to_string());
            tag(&mut x, "precioUnitario", &cents(l.precio_unitario));
            tag(&mut x, "descuento", &cents(l.descuento));
            tag(&mut x, "precioTotalSinImpuesto", &cents(l.base()));
            x.push_str("<impuestos><impuesto>");
            tag(&mut x, "codigo", "2");
            tag(&mut x, "codigoPorcentaje", l.iva.codigo_porcentaje());
            tag(&mut x, "tarifa", &format!("{}.00", l.iva.tarifa()));
            tag(&mut x, "baseImponible", &cents(l.base()));
            tag(&mut x, "valor", &cents(l.iva_valor()));
            x.push_str("</impuesto></impuestos></detalle>");
        }
        x.push_str("</detalles>");
        if !self.info_adicional.is_empty() {
            x.push_str("<infoAdicional>");
            for (n, v) in &self.info_adicional {
                x.push_str(&format!(
                    r#"<campoAdicional nombre="{}">{}</campoAdicional>"#,
                    esc_attr(n),
                    esc(v)
                ));
            }
            x.push_str("</infoAdicional>");
        }
        x.push_str("</factura>");
        Ok((x, clave))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn ejemplo() -> Factura {
        Factura {
            ambiente: Ambiente::Pruebas,
            emisor: Emisor {
                ruc: "1760013210001".into(),
                razon_social: "PRUEBAS & CIA".into(),
                nombre_comercial: None,
                dir_matriz: "QUITO".into(),
                dir_establecimiento: None,
                obligado_contabilidad: false,
                estab: "001".into(),
                pto_emi: "001".into(),
            },
            comprador: Comprador::consumidor_final(),
            fecha_emision: "20112026".into(),
            secuencial: 1,
            codigo_numerico: "12345678".into(),
            lineas: vec![Linea {
                codigo_principal: "S/C".into(),
                descripcion: "ASISTENCIA".into(),
                cantidad: 1,
                precio_unitario: 1500,
                descuento: 0,
                iva: Iva::Quince,
            }],
            forma_pago: "01".into(),
            info_adicional: vec![],
        }
    }

    #[test]
    fn info_adicional_se_escapa_y_va_al_final() {
        let mut f = ejemplo();
        f.info_adicional = vec![("Dirección \"A\"".into(), "Calle 1 & 2".into()), ("Email".into(), "a@b.com".into())];
        let (xml, _) = f.to_xml().unwrap();
        assert!(xml.ends_with(r#"<campoAdicional nombre="Email">a@b.com</campoAdicional></infoAdicional></factura>"#));
        assert!(xml.contains(r#"nombre="Dirección &quot;A&quot;">Calle 1 &amp; 2"#));
        roxmltree::Document::parse(&xml).unwrap();
        f.info_adicional = (0..16).map(|i| (format!("c{i}"), "v".into())).collect();
        assert!(f.to_xml().is_err());
    }

    #[test]
    fn totales_iva_15() {
        let (xml, clave) = ejemplo().to_xml().unwrap();
        assert!(clave_acceso::validar(&clave));
        assert!(xml.contains("<importeTotal>17.25</importeTotal>"));
        assert_eq!(ejemplo().importe_total(), 1725);
        assert_eq!(ejemplo().fecha_iso(), "2026-11-20");
        assert!(xml.contains("<valor>2.25</valor>"));
        assert!(xml.contains("PRUEBAS &amp; CIA"));
        roxmltree::Document::parse(&xml).unwrap();
    }
}

#[cfg(test)]
mod emisor_tests {
    use super::*;

    #[test]
    fn validar_emisor() {
        let mut e = Emisor {
            ruc: "1400544589001".into(),
            razon_social: "X".into(),
            nombre_comercial: None,
            dir_matriz: "Y".into(),
            dir_establecimiento: None,
            obligado_contabilidad: false,
            estab: "001".into(),
            pto_emi: "001".into(),
        };
        assert!(e.validar().is_ok());
        e.ruc = "123".into();
        assert!(e.validar().is_err());
    }
}
