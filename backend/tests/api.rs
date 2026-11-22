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
    accounts::Role,
    api::{router, AppState},
    invoice::Issuer,
    service::Service,
    store::Store,
};
use std::sync::Arc;
use tower::ServiceExt;

const ADMIN_PASSWORD: &str = "admin-password-123";

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

/// SRI en línea simulado: conoce el RUC 1790011674001 (activo) y el 1790011675001 (suspendido).
async fn mock_catastro() -> String {
    use axum::{extract::Query, routing::get, Json};
    use std::collections::HashMap;
    let app = Router::new()
        .route(
            "/ConsolidadoContribuyente/obtenerPorNumerosRuc",
            get(|Query(q): Query<HashMap<String, String>>| async move {
                let ruc = q.get("ruc").cloned().unwrap_or_default();
                Json(match ruc.as_str() {
                    "1790011674001" | "1790011677001" => serde_json::json!([{
                        "numeroRuc": ruc, "razonSocial": "ANA PEREZ", "nombreComercial": "TIENDA ANA",
                        "estadoContribuyenteRuc": "ACTIVO", "obligadoLlevarContabilidad": "SI"
                    }]),
                    "1790011675001" => serde_json::json!([{
                        "numeroRuc": ruc, "razonSocial": "SUSPENDIDO SA",
                        "estadoContribuyenteRuc": "SUSPENDIDO"
                    }]),
                    _ => serde_json::json!([]),
                })
            }),
        )
        .route(
            "/Establecimiento/consultarPorNumeroRuc",
            get(|| async {
                Json(serde_json::json!([
                    {"numeroEstablecimiento": "002", "direccionCompleta": "SUCURSAL", "matriz": "NO"},
                    {"numeroEstablecimiento": "001", "direccionCompleta": "AV. AMAZONAS Y NNUU, QUITO", "matriz": "SI"}
                ]))
            }),
        );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
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

/// The test administrator's id (the data of every test belongs to it).
fn owner_id(st: &AppState) -> i64 {
    st.service.store.find_login("admin").unwrap().unwrap().0.id
}

/// Servicio ya configurado (emisor + certificado) apuntando a un SRI simulado.
async fn app(sri_base: String) -> (Router, Arc<AppState>, String) {
    let (r, st, token) = app_unconfigured(sri_base).await;
    st.service
        .store
        .save_issuer(owner_id(&st), &issuer(), Environment::Testing)
        .unwrap();
    st.service
        .store
        .save_certificate(owner_id(&st), st.service.encryption_key(), P12, "test1234")
        .unwrap();
    (r, st, token)
}

/// App without issuer or certificate, plus the token of a signed-in administrator.
async fn app_unconfigured(sri_base: String) -> (Router, Arc<AppState>, String) {
    let service = Service::new(Store::in_memory().unwrap(), "clave-de-cifrado-de-prueba")
        .con_sri_base(sri_base)
        .with_catastro_base(mock_catastro().await);
    let st = Arc::new(AppState::new(service));
    st.service
        .store
        .create_user("admin", ADMIN_PASSWORD, Role::Admin)
        .unwrap();
    let app = router(st.clone());
    let token = login(&app, "admin", ADMIN_PASSWORD).await.1["token"]
        .as_str()
        .unwrap()
        .to_string();
    (app, st, token)
}

async fn login(app: &Router, user: &str, password: &str) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({ "username": user, "password": password }).to_string();
    json(
        app.clone()
            .oneshot(req("POST", "/auth/login", None, Some(&body)))
            .await
            .unwrap(),
    )
    .await
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
    let (app, _, _) = app("http://127.0.0.1:1".into()).await;
    let r = app
        .oneshot(req("GET", "/health", None, None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
}

#[tokio::test]
async fn missing_or_wrong_token_is_401() {
    let (app, _, _) = app("http://127.0.0.1:1".into()).await;
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
    let (app, _, token) = app(mock_sri(false).await).await;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/invoices", Some(&token), Some(INVOICE_JSON)))
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
                Some(&token),
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
            .oneshot(req("GET", "/invoices", Some(&token), None))
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
        let token = token.clone();
        async move {
            json(
                app.oneshot(req("GET", &uri, Some(&token), None))
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
        .oneshot(req("GET", "/invoices/report.csv", Some(&token), None))
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
        app.oneshot(req("POST", "/invoices", Some(&token), Some(INVOICE_JSON)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(v2["sequence"], 2);
}

#[tokio::test]
async fn invalid_data_is_400_and_does_not_spend_sequence() {
    let (app, st, token) = app(mock_sri(false).await).await;
    let no_lines = r#"{"lines":[]}"#;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/invoices", Some(&token), Some(no_lines)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    let r = app
        .oneshot(req("POST", "/invoices", Some(&token), Some("no es json")))
        .await
        .unwrap();
    assert!(r.status().is_client_error());
    assert_eq!(
        st.service
            .store
            .reserve_sequence(owner_id(&st), Environment::Testing, "001", "001")
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn sri_down_leaves_pending_202() {
    let (app, _, token) = app(mock_sri(true).await).await;
    let (s, v) = json(
        app.oneshot(req("POST", "/invoices", Some(&token), Some(INVOICE_JSON)))
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
    let (app, _, token) = app("http://127.0.0.1:1".into()).await;
    let r = app
        .oneshot(req("GET", "/invoices/0000", Some(&token), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

// ---------- configuración ----------

fn put(token: &str, uri: &str, body: &str) -> Request<Body> {
    req("PUT", uri, Some(token), Some(body))
}

#[tokio::test]
async fn unconfigured_cannot_issue() {
    let (app, st, token) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/invoices", Some(&token), Some(INVOICE_JSON)))
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
            .reserve_sequence(owner_id(&st), Environment::Testing, "001", "001")
            .unwrap(),
        1
    );
    let (_, c) = json(
        app.oneshot(req("GET", "/config", Some(&token), None))
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
    let (app, _, token) = app_unconfigured(mock_sri(false).await).await;
    let issuer = r#"{"ruc":"1760013210001","business_name":"  MI EMPRESA ","head_office_address":"QUITO","establishment":"001","emission_point":"001","accounting_required":true,"trade_name":"","environment":"test"}"#;
    assert_eq!(
        app.clone()
            .oneshot(put(&token, "/config/issuer", issuer))
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
            .oneshot(put(&token, "/config/certificate", &wrong))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let (_, c) = json(
        app.clone()
            .oneshot(req("GET", "/config", Some(&token), None))
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
            .oneshot(put(&token, "/config/certificate", &good))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{info}");
    assert!(info["subject"].as_str().unwrap().contains("PRUEBA FIRMA"));

    let (_, c) = json(
        app.clone()
            .oneshot(req("GET", "/config", Some(&token), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(c["ready"], true);
    assert_eq!(c["issuer"]["business_name"], "MI EMPRESA"); // recortado
    assert_eq!(c["issuer"]["trade_name"], serde_json::Value::Null);
    assert!(!c.to_string().contains("test1234")); // la contraseña nunca vuelve

    let (s, v) = json(
        app.oneshot(req("POST", "/invoices", Some(&token), Some(INVOICE_JSON)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
}

#[tokio::test]
async fn invalid_issuer_is_400() {
    let (app, _, token) = app_unconfigured("http://127.0.0.1:1".into()).await;
    for body in [
        r#"{"ruc":"123","business_name":"X","head_office_address":"Y","establishment":"001","emission_point":"001","environment":"test"}"#,
        r#"{"ruc":"1760013210001","business_name":"  ","head_office_address":"Y","establishment":"001","emission_point":"001","environment":"test"}"#,
        r#"{"ruc":"1760013210001","business_name":"X","head_office_address":"Y","establishment":"001","emission_point":"001","environment":"otro"}"#,
    ] {
        assert_eq!(
            app.clone()
                .oneshot(put(&token, "/config/issuer", body))
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
    let (app, _, token) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let new_product = r#"{"main_code":" A1 ","name":"Camiseta","price_cents":1500,"vat":"15"}"#;
    let (s, p) = json(
        app.clone()
            .oneshot(req("POST", "/products", Some(&token), Some(new_product)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{p}");
    assert_eq!(p["main_code"], "A1");
    let id = p["id"].as_i64().unwrap();

    let (s, _) = json(
        app.clone()
            .oneshot(req("POST", "/products", Some(&token), Some(new_product)))
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
                .oneshot(req("POST", "/products", Some(&token), Some(invalid_body)))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{invalid_body}");
    }

    let edit = r#"{"main_code":"A1","name":"Camiseta XL","price_cents":1800,"vat":"0"}"#;
    let (s, p) = json(
        app.clone()
            .oneshot(put(&token, &format!("/products/{id}"), edit))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!((s, p["price_cents"].clone()), (StatusCode::OK, 1800.into()));
    let (_, l) = json(
        app.clone()
            .oneshot(req("GET", "/products?q=xl", Some(&token), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(l["total"], 1);
    let r = app
        .clone()
        .oneshot(req(
            "DELETE",
            &format!("/products/{id}"),
            Some(&token),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let r = app
        .oneshot(req("GET", &format!("/products/{id}"), Some(&token), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn customers_validate_id_number() {
    let (app, _, token) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let post = |b: &'static str| {
        let app = app.clone();
        let token = token.clone();
        async move {
            json(
                app.oneshot(req("POST", "/customers", Some(&token), Some(b)))
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
    let (app, _, token) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let (s, b) = dispatch(app.clone(), Some(&token), "GET", "/config", None)
        .await
        .unwrap();
    assert_eq!(s, 200);
    assert!(b.contains("\"ready\":false"));
    let new_product = r#"{"main_code":"Z9","name":"Zapato","price_cents":900,"vat":"15"}"#;
    let (s, _) = dispatch(
        app.clone(),
        Some(&token),
        "POST",
        "/products",
        Some(new_product.into()),
    )
    .await
    .unwrap();
    assert_eq!(s, 201);
    let (s, b) = dispatch(app.clone(), Some(&token), "GET", "/products?q=zap", None)
        .await
        .unwrap();
    assert!(s == 200 && b.contains("Zapato"));
    // Token incorrecto y rutas raras se rechazan.
    assert_eq!(
        dispatch(app.clone(), Some("otro"), "GET", "/config", None)
            .await
            .unwrap()
            .0,
        401
    );
    assert!(
        dispatch(app.clone(), Some(&token), "GET", "//evil.com/x", None)
            .await
            .is_err()
    );
    assert!(dispatch(app, Some(&token), "NO VALIDO", "/config", None)
        .await
        .is_err());
}

#[tokio::test]
async fn first_run_setup_then_login() {
    let service = Service::new(Store::in_memory().unwrap(), "clave-de-cifrado-de-prueba");
    let app = router(Arc::new(AppState::new(service)));
    let (_, v) = json(
        app.clone()
            .oneshot(req("GET", "/auth/status", None, None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(v["setup_required"], true);

    // Weak passwords are refused; a good one creates the admin and signs in.
    let weak = r#"{"username":"boss","password":"short"}"#;
    let r = app
        .clone()
        .oneshot(req("POST", "/auth/setup", None, Some(weak)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let good = r#"{"username":"boss","password":"long-enough-password"}"#;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/auth/setup", None, Some(good)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["user"]["role"], "admin");
    let token = v["token"].as_str().unwrap().to_string();

    // Setup is closed once a user exists.
    let r = app
        .clone()
        .oneshot(req("POST", "/auth/setup", None, Some(good)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
    let (_, v) = json(
        app.clone()
            .oneshot(req("GET", "/auth/status", None, None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(v["setup_required"], false);

    let (_, me) = json(
        app.clone()
            .oneshot(req("GET", "/auth/me", Some(&token), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me["username"], "boss");
}

#[tokio::test]
async fn login_logout_and_bad_credentials() {
    let (app, _, token) = app_unconfigured("http://127.0.0.1:1".into()).await;
    assert_eq!(
        login(&app, "admin", "wrong-password").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "nobody", ADMIN_PASSWORD).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Usernames are not case sensitive.
    assert_eq!(login(&app, "ADMIN", ADMIN_PASSWORD).await.0, StatusCode::OK);

    let r = app
        .clone()
        .oneshot(req("POST", "/auth/logout", Some(&token), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let r = app
        .clone()
        .oneshot(req("GET", "/auth/me", Some(&token), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn too_many_failed_logins_are_throttled() {
    let (app, _, _) = app_unconfigured("http://127.0.0.1:1".into()).await;
    for _ in 0..5 {
        assert_eq!(
            login(&app, "admin", "bad-password").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    // Even the right password is refused while the lockout lasts.
    assert_eq!(
        login(&app, "admin", ADMIN_PASSWORD).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn regular_accounts_cannot_manage_users() {
    let (app, st, admin) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let body = r#"{"username":"ana","password":"ana-password-1","role":"operator"}"#;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/users", Some(&admin), Some(body)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let ana_id = v["id"].as_i64().unwrap();
    let ana = login(&app, "ana", "ana-password-1").await.1["token"]
        .as_str()
        .unwrap()
        .to_string();

    // Allowed: read config, use catalogs.
    for (m, u, b) in [("GET", "/config", None), ("GET", "/products", None)] {
        let r = app.clone().oneshot(req(m, u, Some(&ana), b)).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK, "{u}");
    }
    // Forbidden: managing users.
    for (m, u) in [("GET", "/users"), ("POST", "/users")] {
        let r = app
            .clone()
            .oneshot(req(m, u, Some(&ana), Some("{}")))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::FORBIDDEN, "{m} {u}");
    }
    // Without a token everything is 401.
    let r = app
        .clone()
        .oneshot(req("GET", "/users", None, None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

    // Deactivating Ana closes her session.
    let r = app
        .clone()
        .oneshot(req(
            "PUT",
            &format!("/users/{ana_id}"),
            Some(&admin),
            Some(r#"{"active":false}"#),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let r = app
        .clone()
        .oneshot(req("GET", "/config", Some(&ana), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        login(&app, "ana", "ana-password-1").await.0,
        StatusCode::UNAUTHORIZED
    );

    // The only administrator cannot be removed.
    let admin_id = st.service.store.find_login("admin").unwrap().unwrap().0.id;
    let r = app
        .clone()
        .oneshot(req(
            "PUT",
            &format!("/users/{admin_id}"),
            Some(&admin),
            Some(r#"{"role":"operator"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn users_can_change_their_own_password() {
    let (app, _, token) = app_unconfigured("http://127.0.0.1:1".into()).await;
    let other = login(&app, "admin", ADMIN_PASSWORD).await.1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let wrong = r#"{"current_password":"nope","new_password":"brand-new-password"}"#;
    let r = app
        .clone()
        .oneshot(req("PUT", "/auth/password", Some(&token), Some(wrong)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let ok =
        format!(r#"{{"current_password":"{ADMIN_PASSWORD}","new_password":"brand-new-password"}}"#);
    let r = app
        .clone()
        .oneshot(req("PUT", "/auth/password", Some(&token), Some(&ok)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    // The current session stays; the other one is closed.
    let r = app
        .clone()
        .oneshot(req("GET", "/auth/me", Some(&token), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let r = app
        .clone()
        .oneshot(req("GET", "/auth/me", Some(&other), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        login(&app, "admin", "brand-new-password").await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn public_registration_creates_active_operators() {
    let service = Service::new(Store::in_memory().unwrap(), "clave-de-cifrado-de-prueba")
        .with_catastro_base(mock_catastro().await);
    let app = router(Arc::new(AppState::new(service)));
    let body = r#"{"ruc":"1790011674001","password":"ana-password-1"}"#;
    // With no users at all, the first RUC sign-up becomes the administrator.
    let first = r#"{"ruc":"1790011677001","password":"first-password-1"}"#;
    let (s0, v0) = json(
        app.clone()
            .oneshot(req("POST", "/auth/register", None, Some(first)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s0, StatusCode::OK, "{v0}");
    assert_eq!(v0["user"]["role"], "admin");
    // Setup is closed now; the next sign-ups are regular accounts.
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/auth/register", None, Some(body)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["user"]["role"], "operator");
    assert_eq!(v["user"]["username"], "1790011674001");
    let token = v["token"].as_str().unwrap();
    // The issuer comes pre-filled with the SRI's real data.
    let (_, cfg) = json(
        app.clone()
            .oneshot(req("GET", "/config", Some(token), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(cfg["issuer"]["business_name"], "ANA PEREZ");
    assert_eq!(cfg["issuer"]["trade_name"], "TIENDA ANA");
    assert_eq!(
        cfg["issuer"]["head_office_address"],
        "AV. AMAZONAS Y NNUU, QUITO"
    );
    assert_eq!(cfg["issuer"]["accounting_required"], true);
    assert_eq!(cfg["certificate"]["loaded"], false);
    let r = app
        .clone()
        .oneshot(req("GET", "/products", Some(token), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let r = app
        .clone()
        .oneshot(req("GET", "/users", Some(token), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    // Taken usernames and weak passwords are rejected.
    let r = app
        .clone()
        .oneshot(req("POST", "/auth/register", None, Some(body)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
    for bad in [
        r#"{"ruc":"1790011676001","password":"short-but-8chars"}"#, // not in the SRI
        r#"{"ruc":"1790011675001","password":"long-enough-password"}"#, // suspended
        r#"{"ruc":"123","password":"long-enough-password"}"#,       // bad shape
        r#"{"ruc":"1790011674001","password":"short"}"#,            // weak password
    ] {
        let r = app
            .clone()
            .oneshot(req("POST", "/auth/register", None, Some(bad)))
            .await
            .unwrap();
        assert!(
            r.status() == StatusCode::BAD_REQUEST || r.status() == StatusCode::CONFLICT,
            "{bad}: {}",
            r.status()
        );
    }
    let r = app
        .clone()
        .oneshot(req(
            "POST",
            "/auth/register",
            None,
            Some(r#"{"ruc":"1790011676001","password":"long-enough-password"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);

    // With the SRI unreachable, sign-up is refused rather than creating unverified accounts.
    let down = Service::new(Store::in_memory().unwrap(), "clave-de-cifrado-de-prueba")
        .with_catastro_base("http://127.0.0.1:1".into());
    let app2 = router(Arc::new(AppState::new(down)));
    let admin = r#"{"username":"boss","password":"long-enough-password"}"#;
    app2.clone()
        .oneshot(req("POST", "/auth/setup", None, Some(admin)))
        .await
        .unwrap();
    let r = app2
        .oneshot(req("POST", "/auth/register", None, Some(body)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn each_account_only_sees_its_own_business() {
    let (app, st, boss) = app(mock_sri(false).await).await;
    // A second business signs up.
    let body = r#"{"ruc":"1790011674001","password":"ana-password-1"}"#;
    let ana = json(
        app.clone()
            .oneshot(req("POST", "/auth/register", None, Some(body)))
            .await
            .unwrap(),
    )
    .await
    .1["token"]
        .as_str()
        .unwrap()
        .to_string();

    // The first business is configured and issues an invoice; the second starts empty.
    let (_, cfg) = json(
        app.clone()
            .oneshot(req("GET", "/config", Some(&ana), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(cfg["ready"], false);
    // Her issuer details came from the SRI, but she has no signature yet.
    assert_eq!(cfg["issuer"]["ruc"], "1790011674001");
    assert_eq!(cfg["certificate"]["loaded"], false);
    let (_, cfg) = json(
        app.clone()
            .oneshot(req("GET", "/config", Some(&boss), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(cfg["ready"], true);

    let invoice = r#"{"lines":[{"main_code":"A","description":"x","quantity":1,"unit_price_cents":1000,"vat":"15"}]}"#;
    let (s, v) = json(
        app.clone()
            .oneshot(req("POST", "/invoices", Some(&boss), Some(invoice)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let key = v["access_key"].as_str().unwrap().to_string();

    // Ana cannot issue (no issuer/certificate yet), list, read or retry Boss's documents.
    let r = app
        .clone()
        .oneshot(req("POST", "/invoices", Some(&ana), Some(invoice)))
        .await
        .unwrap();
    assert!(r.status().is_client_error());
    let (_, list) = json(
        app.clone()
            .oneshot(req("GET", "/invoices", Some(&ana), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(list["total"], 0);
    let r = app
        .clone()
        .oneshot(req("GET", &format!("/invoices/{key}"), Some(&ana), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    let (_, list) = json(
        app.clone()
            .oneshot(req("GET", "/invoices", Some(&boss), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(list["total"], 1);

    // Catalogs are separate too, and the same product code can exist in both.
    let product = r#"{"main_code":"P1","name":"Thing","price_cents":500,"vat":"15"}"#;
    let (s, p) = json(
        app.clone()
            .oneshot(req("POST", "/products", Some(&boss), Some(product)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let id = p["id"].as_i64().unwrap();
    let (_, list) = json(
        app.clone()
            .oneshot(req("GET", "/products", Some(&ana), None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(list["total"], 0);
    let r = app
        .clone()
        .oneshot(req(
            "PUT",
            &format!("/products/{id}"),
            Some(&ana),
            Some(product),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    let r = app
        .clone()
        .oneshot(req("DELETE", &format!("/products/{id}"), Some(&ana), None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    let r = app
        .clone()
        .oneshot(req("POST", "/products", Some(&ana), Some(product)))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);

    // Sequences are per business.
    let ana_id = st
        .service
        .store
        .find_login("1790011674001")
        .unwrap()
        .unwrap()
        .0
        .id;
    let s = &st.service.store;
    assert_eq!(
        s.reserve_sequence(ana_id, Environment::Testing, "001", "001")
            .unwrap(),
        1
    );
}
