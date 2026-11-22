//! Pruebas de la API con un SRI simulado (axum local) y un certificado de prueba.

use axum::{body::Body, http::{Request, StatusCode}, routing::post, Router};
use http_body_util::BodyExt;
use invoice_rust::{
    api::{router, AppState},
    clave_acceso::Ambiente,
    factura::Emisor,
    firma::Firmador,
    servicio::Servicio,
    sri::SriClient,
    store::Store,
};
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "token-de-prueba-0123456789";

const RECIBIDA: &str = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:validarComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.recepcion"><RespuestaRecepcionComprobante><estado>RECIBIDA</estado><comprobantes/></RespuestaRecepcionComprobante></ns2:validarComprobanteResponse></soap:Body></soap:Envelope>"#;
const AUTORIZADO: &str = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:autorizacionComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.autorizacion"><RespuestaAutorizacionComprobante><claveAccesoConsultada>x</claveAccesoConsultada><numeroComprobantes>1</numeroComprobantes><autorizaciones><autorizacion><estado>AUTORIZADO</estado><numeroAutorizacion>NUM123</numeroAutorizacion><fechaAutorizacion>2026-11-22T10:00:00-05:00</fechaAutorizacion><ambiente>PRUEBAS</ambiente></autorizacion></autorizaciones></RespuestaAutorizacionComprobante></ns2:autorizacionComprobanteResponse></soap:Body></soap:Envelope>"#;

/// SRI simulado. Si `caido`, responde 503 con basura (no es XML).
async fn sri_simulado(caido: bool) -> String {
    let app = if caido {
        Router::new().fallback(|| async { (StatusCode::SERVICE_UNAVAILABLE, "no disponible") })
    } else {
        Router::new()
            .route("/comprobantes-electronicos-ws/RecepcionComprobantesOffline", post(|| async { RECIBIDA }))
            .route("/comprobantes-electronicos-ws/AutorizacionComprobantesOffline", post(|| async { AUTORIZADO }))
    };
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/comprobantes-electronicos-ws", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    base
}

async fn app(sri_base: String) -> (Router, Arc<AppState>) {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test.p12")).unwrap();
    let servicio = Servicio {
        store: Store::en_memoria().unwrap(),
        firmador: Firmador::desde_p12(&p12, "test1234").unwrap(),
        sri: SriClient::with_base(sri_base),
        ambiente: Ambiente::Pruebas,
        emisor: Emisor {
            ruc: "1760013210001".into(),
            razon_social: "PRUEBAS".into(),
            nombre_comercial: None,
            dir_matriz: "QUITO".into(),
            dir_establecimiento: None,
            obligado_contabilidad: false,
            estab: "001".into(),
            pto_emi: "001".into(),
        },
    };
    let st = Arc::new(AppState { servicio, token: TOKEN.into() });
    (router(st.clone()), st)
}

fn req(method: &str, uri: &str, token: Option<&str>, body: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    b.body(Body::from(body.unwrap_or("").to_string())).unwrap()
}

async fn json(r: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let s = r.status();
    let b = r.into_body().collect().await.unwrap().to_bytes();
    (s, serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null))
}

const FACTURA: &str = r#"{"lineas":[{"codigo_principal":"S/C","descripcion":"ASISTENCIA","cantidad":1,"precio_unitario_centavos":1500,"iva":"15"}],"info_adicional":[["Email","a@b.com"]]}"#;

#[tokio::test]
async fn health_no_requiere_token() {
    let (app, _) = app("http://127.0.0.1:1".into()).await;
    let r = app.oneshot(req("GET", "/health", None, None)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
}

#[tokio::test]
async fn sin_token_o_token_malo_es_401() {
    let (app, _) = app("http://127.0.0.1:1".into()).await;
    for t in [None, Some("otro"), Some("")] {
        let r = app.clone().oneshot(req("POST", "/facturas", t, Some(FACTURA))).await.unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "token {t:?}");
    }
}

#[tokio::test]
async fn emitir_y_consultar() {
    let (app, _) = app(sri_simulado(false).await).await;
    let (s, v) = json(app.clone().oneshot(req("POST", "/facturas", Some(TOKEN), Some(FACTURA))).await.unwrap()).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["estado"], "AUTORIZADO");
    assert_eq!(v["secuencial"], 1);
    assert_eq!(v["numero_autorizacion"], "NUM123");
    let clave = v["clave_acceso"].as_str().unwrap().to_string();
    assert!(invoice_rust::clave_acceso::validar(&clave));

    let (s, g) = json(app.clone().oneshot(req("GET", &format!("/facturas/{clave}"), Some(TOKEN), None)).await.unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert!(g["xml_firmado"].as_str().unwrap().contains("<ds:Signature"));

    let (s, l) = json(app.clone().oneshot(req("GET", "/facturas", Some(TOKEN), None)).await.unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(l["total"], 1);
    let it = &l["items"][0];
    assert!(it.get("xml_firmado").is_none());
    assert_eq!(it["numero"], "001-001-000000001");
    assert_eq!(it["valor_total"], 1725);
    assert_eq!(it["identificacion"], "9999999999999");
    assert_eq!(it["tipo_comprobante"], "01");

    // Filtros
    let q = |uri: &str| {
        let app = app.clone();
        let uri = uri.to_string();
        async move { json(app.oneshot(req("GET", &uri, Some(TOKEN), None)).await.unwrap()).await }
    };
    assert_eq!(q("/facturas?identificacion=17").await.1["total"], 0);
    assert_eq!(q("/facturas?identificacion=99999&estado=AUTORIZADO").await.1["total"], 1);
    assert_eq!(q("/facturas?desde=2999-01-01").await.1["total"], 0);
    assert_eq!(q("/facturas?desde=ayer").await.0, StatusCode::BAD_REQUEST);
    let r = app.clone().oneshot(req("GET", "/facturas/reporte.csv", Some(TOKEN), None)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(r.headers()["content-type"].to_str().unwrap().starts_with("text/csv"));
    let csv = String::from_utf8(r.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert_eq!(csv.lines().count(), 2);
    assert!(csv.contains("\"001-001-000000001\"") && csv.contains("\"17.25\""));

    // El secuencial avanza.
    let (_, v2) = json(app.oneshot(req("POST", "/facturas", Some(TOKEN), Some(FACTURA))).await.unwrap()).await;
    assert_eq!(v2["secuencial"], 2);
}

#[tokio::test]
async fn datos_invalidos_es_400_y_no_gasta_secuencial() {
    let (app, st) = app(sri_simulado(false).await).await;
    let sin_lineas = r#"{"lineas":[]}"#;
    let (s, v) = json(app.clone().oneshot(req("POST", "/facturas", Some(TOKEN), Some(sin_lineas))).await.unwrap()).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    let r = app.oneshot(req("POST", "/facturas", Some(TOKEN), Some("no es json"))).await.unwrap();
    assert!(r.status().is_client_error());
    assert_eq!(st.servicio.store.reservar_secuencial(Ambiente::Pruebas, "001", "001").unwrap(), 1);
}

#[tokio::test]
async fn sri_caido_deja_pendiente_202() {
    let (app, _) = app(sri_simulado(true).await).await;
    let (s, v) = json(app.oneshot(req("POST", "/facturas", Some(TOKEN), Some(FACTURA))).await.unwrap()).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["estado"], "PENDIENTE");
    assert!(v["clave_acceso"].as_str().is_some());
}

#[tokio::test]
async fn inexistente_es_404() {
    let (app, _) = app("http://127.0.0.1:1".into()).await;
    let r = app.oneshot(req("GET", "/facturas/0000", Some(TOKEN), None)).await.unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}
