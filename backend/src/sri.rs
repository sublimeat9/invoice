//! Cliente SOAP de los servicios offline del SRI (ficha técnica v2.34, sec. 7).

use crate::access_key::Environment;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum SriError {
    #[error("error HTTP: {0}")]
    Http(#[from] reqwest::Error),
    #[error("XML de respuesta inválido: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("respuesta inesperada del SRI: {0}")]
    Unexpected(String),
    #[error("tiempo de espera agotado sin autorización")]
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub identifier: String,
    pub message: String,
    pub additional_information: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceptionResponse {
    /// `RECEIVED_XML` o `DEVUELTA`.
    pub status: String,
    pub messages: Vec<Message>,
}

impl ReceptionResponse {
    pub fn received(&self) -> bool {
        self.status == "RECIBIDA"
    }

    /// `DEVUELTA` solo porque la clave ya estaba registrada: p. ej. un reintento tras
    /// perder la respuesta de una recepción anterior. Basta con consultar la autorización.
    pub fn already_registered(&self) -> bool {
        self.status == "DEVUELTA"
            && !self.messages.is_empty()
            && self
                .messages
                .iter()
                .all(|m| m.identifier == ACCESS_KEY_REGISTERED_ID)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    /// `AUTHORIZED_XML`, `RECHAZADO`, `NO AUTHORIZED_XML`, `EN PROCESO`...
    pub status: String,
    pub authorization_number: Option<String>,
    pub authorization_date: Option<String>,
    pub environment: Option<String>,
    pub document: Option<String>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationResponse {
    pub access_key: String,
    pub authorizations: Vec<Authorization>,
}

const RETRIES: u32 = 3;

/// Identificador 43: "CLAVE ACCESO REGISTRADA" (el SRI ya recibió este comprobante).
const ACCESS_KEY_REGISTERED_ID: &str = "43";

pub struct SriClient {
    http: reqwest::Client,
    base: String,
}

impl SriClient {
    pub fn new(environment: Environment) -> Self {
        let host = match environment {
            Environment::Testing => "celcer.sri.gob.ec",
            Environment::Production => "cel.sri.gob.ec",
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
    async fn soap(&self, service: &str, body: String) -> Result<String, SriError> {
        let mut delay = Duration::from_secs(1);
        for _ in 0..RETRIES - 1 {
            match self.soap_once(service, &body).await {
                Err(SriError::Http(e)) if e.is_timeout() || e.is_connect() || e.is_request() => {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
                r => return r,
            }
        }
        self.soap_once(service, &body).await
    }

    async fn soap_once(&self, service: &str, body: &str) -> Result<String, SriError> {
        let url = format!("{}/{service}", self.base);
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
    pub async fn reception(&self, signed_xml: &[u8]) -> Result<ReceptionResponse, SriError> {
        let body = reception_envelope(signed_xml);
        let txt = self.soap("RecepcionComprobantesOffline", body).await?;
        parse_reception(&txt)
    }

    pub async fn authorization(&self, access_key: &str) -> Result<AuthorizationResponse, SriError> {
        let body = authorization_envelope(access_key);
        let txt = self.soap("AutorizacionComprobantesOffline", body).await?;
        parse_authorization(&txt)
    }

    /// Consulta la autorización cada `interval` hasta obtener un estado final
    /// o agotar `max_attempts` (el SRI no notifica).
    pub async fn wait_for_authorization(
        &self,
        access_key: &str,
        interval: Duration,
        max_attempts: u32,
    ) -> Result<Authorization, SriError> {
        let mut last_error = None;
        for _ in 0..max_attempts {
            match self.authorization(access_key).await {
                Ok(r) => {
                    last_error = None;
                    if let Some(a) = r.authorizations.into_iter().next() {
                        if a.status != "EN PROCESO" {
                            return Ok(a);
                        }
                    }
                }
                // Fallos transitorios del SRI (p. ej. EntityNotFoundException mientras
                // el comprobante aún se registra): se sigue intentando.
                Err(e) => last_error = Some(e),
            }
            tokio::time::sleep(interval).await;
        }
        Err(last_error.unwrap_or(SriError::Timeout))
    }

    /// Flujo completo: recepción + polling de autorización.
    pub async fn send_and_authorize(
        &self,
        signed_xml: &[u8],
        access_key: &str,
    ) -> Result<Authorization, SriError> {
        let reception = self.reception(signed_xml).await?;
        if !reception.received() && !reception.already_registered() {
            return Err(SriError::Unexpected(format!("{reception:?}")));
        }
        self.wait_for_authorization(access_key, Duration::from_secs(3), 10)
            .await
    }
}

fn reception_envelope(xml: &[u8]) -> String {
    format!(
        r#"<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ec="http://ec.gob.sri.ws.recepcion"><soapenv:Header/><soapenv:Body><ec:validarComprobante><xml>{}</xml></ec:validarComprobante></soapenv:Body></soapenv:Envelope>"#,
        STANDARD.encode(xml)
    )
}

fn authorization_envelope(access_key: &str) -> String {
    format!(
        r#"<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ec="http://ec.gob.sri.ws.autorizacion"><soapenv:Header/><soapenv:Body><ec:autorizacionComprobante><claveAccesoComprobante>{access_key}</claveAccesoComprobante></ec:autorizacionComprobante></soapenv:Body></soapenv:Envelope>"#
    )
}

type Node<'a, 'b> = roxmltree::Node<'a, 'b>;

fn child<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn text(n: Node, name: &str) -> Option<String> {
    child(n, name).map(|c| c.text().unwrap_or("").trim().to_string())
}

fn messages(n: Node) -> Vec<Message> {
    let Some(ms) = child(n, "mensajes") else {
        return vec![];
    };
    ms.children()
        .filter(|c| c.is_element() && c.tag_name().name() == "mensaje")
        .map(|m| Message {
            identifier: text(m, "identificador").unwrap_or_default(),
            message: text(m, "mensaje").unwrap_or_default(),
            additional_information: text(m, "informacionAdicional"),
            kind: text(m, "tipo").unwrap_or_default(),
        })
        .collect()
}

fn fault(doc: &roxmltree::Document) -> Option<String> {
    doc.descendants()
        .find(|n| n.tag_name().name() == "Fault")
        .map(|f| text(f, "faultstring").unwrap_or_else(|| "SOAP Fault".into()))
}

pub fn parse_reception(xml: &str) -> Result<ReceptionResponse, SriError> {
    let doc = roxmltree::Document::parse(xml)?;
    if let Some(f) = fault(&doc) {
        return Err(SriError::Unexpected(f));
    }
    let r = doc
        .descendants()
        .find(|n| n.tag_name().name() == "RespuestaRecepcionComprobante")
        .ok_or_else(|| SriError::Unexpected(xml.chars().take(300).collect()))?;
    let status = text(r, "estado").unwrap_or_default();
    let mut msgs = vec![];
    if let Some(cs) = child(r, "comprobantes") {
        for c in cs.children().filter(|c| c.is_element()) {
            msgs.extend(messages(c));
        }
    }
    Ok(ReceptionResponse {
        status,
        messages: msgs,
    })
}

pub fn parse_authorization(xml: &str) -> Result<AuthorizationResponse, SriError> {
    let doc = roxmltree::Document::parse(xml)?;
    if let Some(f) = fault(&doc) {
        return Err(SriError::Unexpected(f));
    }
    let r = doc
        .descendants()
        .find(|n| n.tag_name().name() == "RespuestaAutorizacionComprobante")
        .ok_or_else(|| SriError::Unexpected(xml.chars().take(300).collect()))?;
    let authorizations = child(r, "autorizaciones")
        .map(|a| {
            a.children()
                .filter(|c| c.is_element() && c.tag_name().name() == "autorizacion")
                .map(|a| Authorization {
                    status: text(a, "estado").unwrap_or_default(),
                    authorization_number: text(a, "numeroAutorizacion"),
                    authorization_date: text(a, "fechaAutorizacion"),
                    environment: text(a, "ambiente"),
                    document: text(a, "comprobante"),
                    messages: messages(a),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(AuthorizationResponse {
        access_key: text(r, "claveAccesoConsultada").unwrap_or_default(),
        authorizations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reception_received() {
        let x = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:validarComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.recepcion"><RespuestaRecepcionComprobante><estado>RECIBIDA</estado><comprobantes/></RespuestaRecepcionComprobante></ns2:validarComprobanteResponse></soap:Body></soap:Envelope>"#;
        let r = parse_reception(x).unwrap();
        assert!(r.received());
        assert!(r.messages.is_empty());
    }

    #[test]
    fn access_key_already_registered() {
        let r = |id: &str| ReceptionResponse {
            status: "DEVUELTA".into(),
            messages: vec![Message {
                identifier: id.into(),
                message: String::new(),
                additional_information: None,
                kind: "ERROR".into(),
            }],
        };
        assert!(r("43").already_registered());
        assert!(!r("35").already_registered());
    }

    #[test]
    fn reception_returned() {
        let x = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:validarComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.recepcion"><RespuestaRecepcionComprobante><estado>DEVUELTA</estado><comprobantes><comprobante><claveAcceso>1</claveAcceso><mensajes><mensaje><identificador>35</identificador><mensaje>DOCUMENTO INVÁLIDO</mensaje><informacionAdicional>x</informacionAdicional><tipo>ERROR</tipo></mensaje></mensajes></comprobante></comprobantes></RespuestaRecepcionComprobante></ns2:validarComprobanteResponse></soap:Body></soap:Envelope>"#;
        let r = parse_reception(x).unwrap();
        assert!(!r.received());
        assert_eq!(r.messages[0].identifier, "35");
        assert_eq!(r.messages[0].kind, "ERROR");
    }

    #[test]
    fn authorization_authorized() {
        let x = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:autorizacionComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.autorizacion"><RespuestaAutorizacionComprobante><claveAccesoConsultada>
 0503201201176001321000110010030009900641234567814
 </claveAccesoConsultada><numeroComprobantes>1</numeroComprobantes><autorizaciones><autorizacion><estado>AUTORIZADO</estado><numeroAutorizacion>0503</numeroAutorizacion><fechaAutorizacion>2012-03-05T16:57:34.997-05:00</fechaAutorizacion><ambiente>PRUEBAS</ambiente><comprobante><![CDATA[<factura/>]]></comprobante><mensajes><mensaje><identificador>60</identificador><mensaje>PRUEBAS</mensaje><tipo>ADVERTENCIA</tipo></mensaje></mensajes></autorizacion></autorizaciones></RespuestaAutorizacionComprobante></ns2:autorizacionComprobanteResponse></soap:Body></soap:Envelope>"#;
        let r = parse_authorization(x).unwrap();
        assert_eq!(
            r.access_key,
            "0503201201176001321000110010030009900641234567814"
        );
        let a = &r.authorizations[0];
        assert_eq!(a.status, "AUTORIZADO");
        assert_eq!(a.document.as_deref(), Some("<factura/>"));
        assert_eq!(a.messages[0].kind, "ADVERTENCIA");
    }

    #[test]
    fn no_authorizations_yet() {
        let x = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:autorizacionComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.autorizacion"><RespuestaAutorizacionComprobante><claveAccesoConsultada>1</claveAccesoConsultada><numeroComprobantes>0</numeroComprobantes><autorizaciones/></RespuestaAutorizacionComprobante></ns2:autorizacionComprobanteResponse></soap:Body></soap:Envelope>"#;
        assert!(parse_authorization(x).unwrap().authorizations.is_empty());
    }
}
