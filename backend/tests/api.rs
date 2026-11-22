//! Pruebas de la API con un SRI simulado (axum local) y un certificado de prueba.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use http_body_util::BodyExt;
use invoice_rust::{
    access_key::Environment,
    api::{router, AppState},
    invoice::Issuer,
    service::Service,
    store::Store,
};
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "token-de-prueba-0123456789";

const RECEIVED_XML: &str = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:validarComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.recepcion"><RespuestaRecepcionComprobante><estado>RECIBIDA</estado><comprobantes/></RespuestaRecepcionComprobante></ns2:validarComprobanteResponse></soap:Body></soap:Envelope>"#;
const AUTHORIZED_XML: &str = r#"<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns2:autorizacionComprobanteResponse xmlns:ns2="http://ec.gob.sri.ws.autorizacion"><RespuestaAutorizacionComprobante><claveAccesoConsultada>x</claveAccesoConsultada><numeroComprobantes>1</numeroComprobantes><autorizaciones><autorizacion><estado>AUTORIZADO</estado><numeroAutorizacion>NUM123</numeroAutorizacion><fechaAutorizacion>2026-11-22T10:00:00-05:00</fechaAutorizacion><ambiente>PRUEBAS</ambiente></autorizacion></autorizaciones></RespuestaAutorizacionComprobante></ns2:autorizacionComprobanteResponse></soap:Body></soap:Envelope>"#;

/// SRI simulado. Si `down`, responde 503 con basura (no es XML).
async fn mock_sri(down: bool) -> String {
    let app = if down {
        Router::new().fallback(|| async { (StatusCode::SERVICE_UNAVAILABLE, "no disponible") })
    } else {
        Router::new()
            .route(
                "/comprobantes-electronicos-ws/RecepcionComprobantesOffline",
                post(|| async { RECEIVED_XML }),
            )
            .route(
                "/comprobantes-electronicos-ws/AutorizacionComprobantesOffline",
                post(|| async { AUTHORIZED_XML }),
            )
    };
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "http://{}/comprobantes-electronicos-ws",
        l.local_addr().unwrap()
    );
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    base
}

const P12: &[u8] = include_bytes!("fixtures/test.p12");

fn issuer() -> Issuer {
    Issuer {
        ruc: "1760013210001".into(),
        business_name: "PRUEBAS".into(),
        trade_name: None,
        head_office_address: "QUITO".into(),
        branch_address: None,
        accounting_required: false,
        establishment: "001".into(),
        emission_point: "001".into(),
    }
}

/// Servicio ya configurado (emisor + certificado) apuntando a un SRI simulado.
async fn app(sri_base: String) -> (Router, Arc<AppState>) {
    let (r, st) = app_unconfigured(sri_base).await;
    st.service
        .store
        .save_issuer(&issuer(), Environment::Testing)
        .unwrap();
    st.service
        .store
        .save_certificate(st.service.encryption_key(), P12, "test1234")
        .unwrap();
    (r, st)
}

async fn app_unconfigured(sri_base: String) -> (Router, Arc<AppState>) {
    let service = Service::new(Store::in_memory().unwrap(), "clave-de-cifrado-de-prueba")
        .con_sri_base(sri_base);
    let st = Arc::new(AppState {
        service,
        token: TOKEN.into(),
    });
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
    (
        s,
        serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null),
    )
}

const INVOICE_JSON: &str = r#"{"lines":[{"main_code":"S/C","description":"ASISTENCIA","quantity":1,"unit_price_cents":1500,"vat":"15"}],"additional_info":[["Email","a@b.com"]]}"#;

#[tokio::test]
async fn health_requires_no_token() {
    let (app, _) = app("http://127.0.0.1:1".into()).await;
    let r = app
        .oneshot(req("GET", "/health", None, None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
}

#[tokio::test]
async fn missing_or_wrong_token_is_401() {
    let (app, _) = app("http://127.0.0.1:1".into()).await;
    for t in [None, Some("otro"), Some("")] {
        let r = app
            .clone()
            .oneshot(req("POST", "/invoices", t, Some(INVOICE_JSON)))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "token {t:?}");
    }
}

#[tokio::test]
async fn issue_and_query() {
    let (app, _) = app(mock_sri(false).await).await;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/invoices", Some(TOKEN), Some(INVOICE_JSON)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["status"], "AUTORIZADO");
    assert_eq!(v["sequence"], 1);
    assert_eq!(v["authorization_number"], "NUM123");
    let access_key = v["access_key"].as_str().unwrap().to_string();
    assert!(invoice_rust::access_key::validate(&access_key));

    let (s, g) = json(
        app.clone()
            .oneshot(req(
                "GET",
                &format!("/invoices/{access_key}"),
                Some(TOKEN),
                None,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(g["signed_xml"].as_str().unwrap().contains("<ds:Signature"));

    let (s, l) = json(
        app.clone()
            .oneshot(req("GET", "/invoices", Some(TOKEN), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(l["total"], 1);
    let it = &l["items"][0];
    assert!(it.get("signed_xml").is_none());
    assert_eq!(it["number"], "001-001-000000001");
    assert_eq!(it["total_amount"], 1725);
    assert_eq!(it["id_number"], "9999999999999");
    assert_eq!(it["document_type"], "01");

    // Filtros
    let q = |uri: &str| {
        let app = app.clone();
        let uri = uri.to_string();
        async move {
            json(
                app.oneshot(req("GET", &uri, Some(TOKEN), None))
                    .await
                    .unwrap(),
            )
            .await
        }
    };
    assert_eq!(q("/invoices?id_number=17").await.1["total"], 0);
    assert_eq!(
        q("/invoices?id_number=99999&status=AUTORIZADO").await.1["total"],
        1
    );
    assert_eq!(q("/invoices?date_from=2999-01-01").await.1["total"], 0);
    assert_eq!(
        q("/invoices?date_from=ayer").await.0,
        StatusCode::BAD_REQUEST
    );
    let r = app
        .clone()
        .oneshot(req("GET", "/invoices/report.csv", Some(TOKEN), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(r.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/csv"));
    let csv =
        String::from_utf8(r.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert_eq!(csv.lines().count(), 2);
    assert!(csv.contains("\"001-001-000000001\"") && csv.contains("\"17.25\""));

    // El secuencial avanza.
    let (_, v2) = json(
        app.oneshot(req("POST", "/invoices", Some(TOKEN), Some(INVOICE_JSON)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(v2["sequence"], 2);
}

#[tokio::test]
async fn invalid_data_is_400_and_does_not_spend_sequence() {
    let (app, st) = app(mock_sri(false).await).await;
    let no_lines = r#"{"lines":[]}"#;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/invoices", Some(TOKEN), Some(no_lines)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    let r = app
        .oneshot(req("POST", "/invoices", Some(TOKEN), Some("no es json")))
        .await
        .unwrap();
    assert!(r.status().is_client_error());
    assert_eq!(
        st.service
            .store
            .reserve_sequence(Environment::Testing, "001", "001")
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn sri_down_leaves_pending_202() {
    let (app, _) = app(mock_sri(true).await).await;
    let (s, v) = json(
        app.oneshot(req("POST", "/invoices", Some(TOKEN), Some(INVOICE_JSON)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["status"], "PENDIENTE");
    assert!(v["access_key"].as_str().is_some());
}

#[tokio::test]
async fn missing_is_404() {
    let (app, _) = app("http://127.0.0.1:1".into()).await;
    let r = app
        .oneshot(req("GET", "/invoices/0000", Some(TOKEN), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

// ---------- configuración ----------

fn put(uri: &str, body: &str) -> Request<Body> {
    req("PUT", uri, Some(TOKEN), Some(body))
}

#[tokio::test]
async fn unconfigured_cannot_issue() {
    let (app, st) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/invoices", Some(TOKEN), Some(INVOICE_JSON)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    assert!(v["error"].as_str().unwrap().contains("emisor"));
    // No se gastó ningún secuencial.
    assert_eq!(
        st.service
            .store
            .reserve_sequence(Environment::Testing, "001", "001")
            .unwrap(),
        1
    );
    let (_, c) = json(
        app.oneshot(req("GET", "/config", Some(TOKEN), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        (c["ready"].clone(), c["certificate"]["loaded"].clone()),
        (false.into(), false.into())
    );
}

#[tokio::test]
async fn configure_via_api_and_issue() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let (app, _) = app_unconfigured(mock_sri(false).await).await;
    let issuer = r#"{"ruc":"1760013210001","business_name":"  MI EMPRESA ","head_office_address":"QUITO","establishment":"001","emission_point":"001","accounting_required":true,"trade_name":"","environment":"test"}"#;
    assert_eq!(
        app.clone()
            .oneshot(put("/config/issuer", issuer))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );

    // Contraseña incorrecta: rechazada y no se guarda.
    let wrong = format!(
        r#"{{"p12_base64":"{}","password":"incorrecta"}}"#,
        STANDARD.encode(P12)
    );
    assert_eq!(
        app.clone()
            .oneshot(put("/config/certificate", &wrong))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let (_, c) = json(
        app.clone()
            .oneshot(req("GET", "/config", Some(TOKEN), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(c["certificate"]["loaded"], false);

    let good = format!(
        r#"{{"p12_base64":"{}","password":"test1234"}}"#,
        STANDARD.encode(P12)
    );
    let (s, info) = json(
        app.clone()
            .oneshot(put("/config/certificate", &good))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{info}");
    assert!(info["subject"].as_str().unwrap().contains("PRUEBA FIRMA"));

    let (_, c) = json(
        app.clone()
            .oneshot(req("GET", "/config", Some(TOKEN), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(c["ready"], true);
    assert_eq!(c["issuer"]["business_name"], "MI EMPRESA"); // recortado
    assert_eq!(c["issuer"]["trade_name"], serde_json::Value::Null);
    assert!(!c.to_string().contains("test1234")); // la contraseña nunca vuelve

    let (s, v) = json(
        app.oneshot(req("POST", "/invoices", Some(TOKEN), Some(INVOICE_JSON)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
}

#[tokio::test]
async fn invalid_issuer_is_400() {
    let (app, _) = app_unconfigured("http://127.0.0.1:1".into()).await;
    for body in [
        r#"{"ruc":"123","business_name":"X","head_office_address":"Y","establishment":"001","emission_point":"001","environment":"test"}"#,
        r#"{"ruc":"1760013210001","business_name":"  ","head_office_address":"Y","establishment":"001","emission_point":"001","environment":"test"}"#,
        r#"{"ruc":"1760013210001","business_name":"X","head_office_address":"Y","establishment":"001","emission_point":"001","environment":"otro"}"#,
    ] {
        assert_eq!(
            app.clone()
                .oneshot(put("/config/issuer", body))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST,
            "{body}"
        );
    }
}

// ---------- catálogos ----------

#[tokio::test]
async fn products_crud() {
    let (app, _) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let new_product = r#"{"main_code":" A1 ","name":"Camiseta","price_cents":1500,"vat":"15"}"#;
    let (s, p) = json(
        app.clone()
            .oneshot(req("POST", "/products", Some(TOKEN), Some(new_product)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{p}");
    assert_eq!(p["main_code"], "A1");
    let id = p["id"].as_i64().unwrap();

    let (s, _) = json(
        app.clone()
            .oneshot(req("POST", "/products", Some(TOKEN), Some(new_product)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    for invalid_body in [
        r#"{"main_code":"","name":"x","price_cents":1,"vat":"15"}"#,
        r#"{"main_code":"B","name":"x","price_cents":-5,"vat":"15"}"#,
        r#"{"main_code":"B","name":"x","price_cents":5,"vat":"12"}"#,
    ] {
        let (s, _) = json(
            app.clone()
                .oneshot(req("POST", "/products", Some(TOKEN), Some(invalid_body)))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{invalid_body}");
    }

    let edit = r#"{"main_code":"A1","name":"Camiseta XL","price_cents":1800,"vat":"0"}"#;
    let (s, p) = json(
        app.clone()
            .oneshot(put(&format!("/products/{id}"), edit))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!((s, p["price_cents"].clone()), (StatusCode::OK, 1800.into()));
    let (_, l) = json(
        app.clone()
            .oneshot(req("GET", "/products?q=xl", Some(TOKEN), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(l["total"], 1);
    let r = app
        .clone()
        .oneshot(req("DELETE", &format!("/products/{id}"), Some(TOKEN), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let r = app
        .oneshot(req("GET", &format!("/products/{id}"), Some(TOKEN), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn customers_validate_id_number() {
    let (app, _) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let post = |b: &'static str| {
        let app = app.clone();
        async move {
            json(
                app.oneshot(req("POST", "/customers", Some(TOKEN), Some(b)))
                    .await
                    .unwrap(),
            )
            .await
            .0
        }
    };
    assert_eq!(
        post(
            r#"{"id_type":"05","id_number":"1712345678","business_name":"Juan","email":"j@x.com"}"#
        )
        .await,
        StatusCode::CREATED
    );
    assert_eq!(
        post(r#"{"id_type":"05","id_number":"1712345678","business_name":"Otro"}"#).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        post(r#"{"id_type":"05","id_number":"123","business_name":"X"}"#).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(r#"{"id_type":"04","id_number":"1760013210001","business_name":"ACME"}"#).await,
        StatusCode::CREATED
    );
    assert_eq!(
        post(r#"{"id_type":"07","id_number":"9999999999999","business_name":"CF"}"#).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(r#"{"id_type":"06","id_number":"AB123","business_name":"X","email":"no-es-correo"}"#)
            .await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn in_process_dispatch_without_network() {
    use invoice_rust::api::dispatch;
    let (app, _) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let (s, b) = dispatch(app.clone(), TOKEN, "GET", "/config", None)
        .await
        .unwrap();
    assert_eq!(s, 200);
    assert!(b.contains("\"ready\":false"));
    let new_product = r#"{"main_code":"Z9","name":"Zapato","price_cents":900,"vat":"15"}"#;
    let (s, _) = dispatch(
        app.clone(),
        TOKEN,
        "POST",
        "/products",
        Some(new_product.into()),
    )
    .await
    .unwrap();
    assert_eq!(s, 201);
    let (s, b) = dispatch(app.clone(), TOKEN, "GET", "/products?q=zap", None)
        .await
        .unwrap();
    assert!(s == 200 && b.contains("Zapato"));
    // Token incorrecto y rutas raras se rechazan.
    assert_eq!(
        dispatch(app.clone(), "otro", "GET", "/config", None)
            .await
            .unwrap()
            .0,
        401
    );
    assert!(dispatch(app.clone(), TOKEN, "GET", "//evil.com/x", None)
        .await
        .is_err());
    assert!(dispatch(app, TOKEN, "NO VALIDO", "/config", None)
        .await
        .is_err());
}
