//! Cliente SOAP de los servicios offline del SRI (ficha técnica v2.34, sec. 7).

use crate::clave_acceso::Ambiente;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum SriError {
    #[error("error HTTP: {0}")]
    Http(#[from] reqwest::Error),
    #[error("XML de respuesta inválido: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("respuesta inesperada del SRI: {0}")]
    Inesperada(String),
    #[error("tiempo de espera agotado sin autorización")]
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mensaje {
    pub identificador: String,
    pub mensaje: String,
    pub informacion_adicional: Option<String>,
    pub tipo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespuestaRecepcion {
    /// `RECIBIDA` o `DEVUELTA`.
    pub estado: String,
    pub mensajes: Vec<Mensaje>,
}

impl RespuestaRecepcion {
    pub fn recibida(&self) -> bool {
        self.estado == "RECIBIDA"
    }

    /// `DEVUELTA` solo porque la clave ya estaba registrada: p. ej. un reintento tras
    /// perder la respuesta de una recepción anterior. Basta con consultar la autorización.
    pub fn ya_registrada(&self) -> bool {
        self.estado == "DEVUELTA"
            && !self.mensajes.is_empty()
            && self.mensajes.iter().all(|m| m.identificador == ID_CLAVE_REGISTRADA)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Autorizacion {
    /// `AUTORIZADO`, `RECHAZADO`, `NO AUTORIZADO`, `EN PROCESO`...
    pub estado: String,
    pub numero_autorizacion: Option<String>,
    pub fecha_autorizacion: Option<String>,
    pub ambiente: Option<String>,
    pub comprobante: Option<String>,
    pub mensajes: Vec<Mensaje>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespuestaAutorizacion {
    pub clave_acceso: String,
    pub autorizaciones: Vec<Autorizacion>,
}

const REINTENTOS: u32 = 3;

/// Identificador 43: "CLAVE ACCESO REGISTRADA" (el SRI ya recibió este comprobante).
const ID_CLAVE_REGISTRADA: &str = "43";

pub struct SriClient {
    http: reqwest::Client,
    base: String,
}

impl SriClient {
    pub fn new(ambiente: Ambiente) -> Self {
        let host = match ambiente {
            Ambiente::Pruebas => "celcer.sri.gob.ec",
            Ambiente::Produccion => "cel.sri.gob.ec",
        };
        Self::with_base(format!("https://{host}/comprobantes-electronicos-ws"))
    }

    /// Útil para apuntar a un servidor de prueba local.
    pub fn with_base(base: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("cliente HTTP");
        Self { http, base }
    }

    /// Reintenta errores de transporte (timeouts, conexión) con espera creciente.
    async fn soap(&self, servicio: &str, body: String) -> Result<String, SriError> {
        let mut espera = Duration::from_secs(1);
        for _ in 0..REINTENTOS - 1 {
            match self.soap_una_vez(servicio, &body).await {
                Err(SriError::Http(e)) if e.is_timeout() || e.is_connect() || e.is_request() => {
                    tokio::time::sleep(espera).await;
                    espera *= 2;
                }
                r => return r,
            }
        }
        self.soap_una_vez(servicio, &body).await
    }

    async fn soap_una_vez(&self, servicio: &str, body: &str) -> Result<String, SriError> {
        let url = format!("{}/{servicio}", self.base);
        let resp = self
            .http
            .post(url)
            .header("Content-Type", "text/xml;charset=UTF-8")
            .header("SOAPAction", "")
            .body(body.to_owned())
            .send()
            .await?;
        // El SRI devuelve 500 con un SOAP Fault; lo dejamos pasar al parser.
        Ok(resp.text().await?)
    }

    /// Envía el XML ya firmado (bytes UTF-8) al servicio de recepción.
    pub async fn recepcion(&self, xml_firmado: &[u8]) -> Result<RespuestaRecepcion, SriError> {
        let body = envelope_recepcion(xml_firmado);
        let txt = self.soap("RecepcionComprobantesOffline", body).await?;
        parse_recepcion(&txt)
    }

    pub async fn autorizacion(&self, clave: &str) -> Result<RespuestaAutorizacion, SriError> {
        let body = envelope_autorizacion(clave);
        let txt = self.soap("AutorizacionComprobantesOffline", body).await?;
        parse_autorizacion(&txt)
    }

    /// Consulta la autorización cada `intervalo` hasta obtener un estado final
    /// o agotar `max_intentos` (el SRI no notifica).
    pub async fn esperar_autorizacion(
        &self,
        clave: &str,
        intervalo: Duration,
        max_intentos: u32,
    ) -> Result<Autorizacion, SriError> {
        let mut ultimo_error = None;
        for _ in 0..max_intentos {
            match self.autorizacion(clave).await {
                Ok(r) => {
                    ultimo_error = None;
                    if let Some(a) = r.autorizaciones.into_iter().next() {
                        if a.estado != "EN PROCESO" {
                            return Ok(a);
                        }
                    }
                }
                // Fallos transitorios del SRI (p. ej. EntityNotFoundException mientras
                // el comprobante aún se registra): se sigue intentando.
                Err(e) => ultimo_error = Some(e),
            }
            tokio::time::sleep(intervalo).await;
        }
        Err(ultimo_error.unwrap_or(SriError::Timeout))
    }

    /// Flujo completo: recepción + polling de autorización.
    pub async fn enviar_y_autorizar(
        &self,
        xml_firmado: &[u8],
        clave: &str,
    ) -> Result<Autorizacion, SriError> {
        let rec = self.recepcion(xml_firmado).await?;
        if !rec.recibida() && !rec.ya_registrada() {
            return Err(SriError::Inesperada(format!("{rec:?}")));
        }
        self.esperar_autorizacion(clave, Duration::from_secs(3), 10).await
    }
}

fn envelope_recepcion(xml: &[u8]) -> String {
    format!(
        r#"<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ec="http://ec.gob.sri.ws.recepcion"><soapenv:Header/><soapenv:Body><ec:validarComprobante><xml>{}</xml></ec:validarComprobante></soapenv:Body></soapenv:Envelope>"#,
        STANDARD.encode(xml)
    )
}

fn envelope_autorizacion(clave: &str) -> String {
    format!(
        r#"<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ec="http://ec.gob.sri.ws.autorizacion"><soapenv:Header/><soapenv:Body><ec:autorizacionComprobante><claveAccesoComprobante>{clave}</claveAccesoComprobante></ec:autorizacionComprobante></soapenv:Body></soapenv:Envelope>"#
    )
}

type Nodo<'a, 'b> = roxmltree::Node<'a, 'b>;

fn hijo<'a, 'b>(n: Nodo<'a, 'b>, nombre: &str) -> Option<Nodo<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == nombre)
}

fn texto(n: Nodo, nombre: &str) -> Option<String> {
    hijo(n, nombre).map(|c| c.text().unwrap_or("").trim().to_string())
}

fn mensajes(n: Nodo) -> Vec<Mensaje> {
    let Some(ms) = hijo(n, "mensajes") else { return vec![] };
    ms.children()
        .filter(|c| c.is_element() && c.tag_name().name() == "mensaje")
        .map(|m| Mensaje {
            identificador: texto(m, "identificador").unwrap_or_default(),
            mensaje: texto(m, "mensaje").unwrap_or_default(),
            informacion_adicional: texto(m, "informacionAdicional"),
            tipo: texto(m, "tipo").unwrap_or_default(),
        })
        .collect()
}

fn fault(doc: &roxmltree::Document) -> Option<String> {
    doc.descendants()
        .find(|n| n.tag_name().name() == "Fault")
        .map(|f| texto(f, "faultstring").unwrap_or_else(|| "SOAP Fault".into()))
}

pub fn parse_recepcion(xml: &str) -> Result<RespuestaRecepcion, SriError> {
    let doc = roxmltree::Document::parse(xml)?;
    if let Some(f) = fault(&doc) {
        return Err(SriError::Inesperada(f));
    }
    let r = doc
        .descendants()
        .find(|n| n.tag_name().name() == "RespuestaRecepcionComprobante")
        .ok_or_else(|| SriError::Inesperada(xml.chars().take(300).collect()))?;
    let estado = texto(r, "estado").unwrap_or_default();
    let mut msgs = vec![];
    if let Some(cs) = hijo(r, "comprobantes") {
        for c in cs.children().filter(|c| c.is_element()) {
            msgs.extend(mensajes(c));
        }
    }
    Ok(RespuestaRecepcion { estado, mensajes: msgs })
}

pub fn parse_autorizacion(xml: &str) -> Result<RespuestaAutorizacion, SriError> {
    let doc = roxmltree::Document::parse(xml)?;
    if let Some(f) = fault(&doc) {
        return Err(SriError::Inesperada(f));
    }
    let r = doc
        .descendants()
        .find(|n| n.tag_name().name() == "RespuestaAutorizacionComprobante")
        .ok_or_else(|| SriError::Inesperada(xml.chars().take(300).collect()))?;
    let autorizaciones = hijo(r, "autorizaciones")
        .map(|a| {
            a.children()
                .filter(|c| c.is_element() && c.tag_name().name() == "autorizacion")
                .map(|a| Autorizacion {
                    estado: texto(a, "estado").unwrap_or_default(),
                    numero_autorizacion: texto(a, "numeroAutorizacion"),
                    fecha_autorizacion: texto(a, "fechaAutorizacion"),
                    ambiente: texto(a, "ambiente"),
                    comprobante: texto(a, "comprobante"),
                    mensajes: mensajes(a),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(RespuestaAutorizacion {
        clave_acceso: texto(r, "claveAccesoConsultada").unwrap_or_default(),
        autorizaciones,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recepcion_recibida() {
        let x = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:validarComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.recepcion"><RespuestaRecepcionComprobante><estado>RECIBIDA</estado><comprobantes/></RespuestaRecepcionComprobante></ns2:validarComprobanteResponse></soap:Body></soap:Envelope>"#;
        let r = parse_recepcion(x).unwrap();
        assert!(r.recibida());
        assert!(r.mensajes.is_empty());
    }

    #[test]
    fn clave_ya_registrada() {
        let r = |id: &str| RespuestaRecepcion {
            estado: "DEVUELTA".into(),
            mensajes: vec![Mensaje { identificador: id.into(), mensaje: String::new(), informacion_adicional: None, tipo: "ERROR".into() }],
        };
        assert!(r("43").ya_registrada());
        assert!(!r("35").ya_registrada());
    }

    #[test]
    fn recepcion_devuelta() {
        let x = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:validarComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.recepcion"><RespuestaRecepcionComprobante><estado>DEVUELTA</estado><comprobantes><comprobante><claveAcceso>1</claveAcceso><mensajes><mensaje><identificador>35</identificador><mensaje>DOCUMENTO INVÁLIDO</mensaje><informacionAdicional>x</informacionAdicional><tipo>ERROR</tipo></mensaje></mensajes></comprobante></comprobantes></RespuestaRecepcionComprobante></ns2:validarComprobanteResponse></soap:Body></soap:Envelope>"#;
        let r = parse_recepcion(x).unwrap();
        assert!(!r.recibida());
        assert_eq!(r.mensajes[0].identificador, "35");
        assert_eq!(r.mensajes[0].tipo, "ERROR");
    }

    #[test]
    fn autorizacion_autorizado() {
        let x = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:autorizacionComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.autorizacion"><RespuestaAutorizacionComprobante><claveAccesoConsultada>
 0503201201176001321000110010030009900641234567814
 </claveAccesoConsultada><numeroComprobantes>1</numeroComprobantes><autorizaciones><autorizacion><estado>AUTORIZADO</estado><numeroAutorizacion>0503</numeroAutorizacion><fechaAutorizacion>2012-03-05T16:57:34.997-05:00</fechaAutorizacion><ambiente>PRUEBAS</ambiente><comprobante><![CDATA[<factura/>]]></comprobante><mensajes><mensaje><identificador>60</identificador><mensaje>PRUEBAS</mensaje><tipo>ADVERTENCIA</tipo></mensaje></mensajes></autorizacion></autorizaciones></RespuestaAutorizacionComprobante></ns2:autorizacionComprobanteResponse></soap:Body></soap:Envelope>"#;
        let r = parse_autorizacion(x).unwrap();
        assert_eq!(r.clave_acceso, "0503201201176001321000110010030009900641234567814");
        let a = &r.autorizaciones[0];
        assert_eq!(a.estado, "AUTORIZADO");
        assert_eq!(a.comprobante.as_deref(), Some("<factura/>"));
        assert_eq!(a.mensajes[0].tipo, "ADVERTENCIA");
    }

    #[test]
    fn sin_autorizaciones_aun() {
        let x = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:autorizacionComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.autorizacion"><RespuestaAutorizacionComprobante><claveAccesoConsultada>1</claveAccesoConsultada><numeroComprobantes>0</numeroComprobantes><autorizaciones/></RespuestaAutorizacionComprobante></ns2:autorizacionComprobanteResponse></soap:Body></soap:Envelope>"#;
        assert!(parse_autorizacion(x).unwrap().autorizaciones.is_empty());
    }
}
