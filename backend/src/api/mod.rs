//! API HTTP (JSON) sobre `Service`. Un módulo por recurso:
//!
//! | módulo       | rutas                                                                  |
//! |--------------|------------------------------------------------------------------------|
//! | `invoices`   | `POST /invoices`, `GET /invoices/{access_key}`, `POST /invoices/retry` |
//! | `search`   | `GET /invoices` (filtros) y `GET /invoices/report.csv`                |
//! | `config`     | `GET /config`, `PUT /config/issuer`, `PUT /config/certificate`         |
//! | `catalogs`  | `/products` y `/customers` (CRUD)                                      |
//! | `auth`       | exige `Authorization: Bearer <API_TOKEN>` (salvo `GET /health`)        |
//! | `dispatch`   | `dispatch`: invoca el router dentro del proceso (app de escritorio)   |
//!
//! Los montos viajan en centavos enteros.

mod auth;
mod catalogs;
mod config;
mod dispatch;
mod errors;
mod invoices;
mod search;

use crate::service::Service;
use axum::{
    middleware,
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;

pub use dispatch::dispatch;

pub struct AppState {
    pub service: Service,
    pub token: String,
}

pub fn router(state: Arc<AppState>) -> Router {
    let protected_routes = Router::new()
        .route("/invoices", post(invoices::issue).get(search::search))
        .route("/invoices/report.csv", get(search::report))
        .route("/invoices/retry", post(invoices::retry))
        .route("/invoices/{access_key}", get(invoices::get_invoice))
        .route("/config", get(config::view_config))
        .route("/config/issuer", put(config::save_issuer))
        .route("/config/certificate", put(config::save_certificate))
        .route(
            "/products",
            get(catalogs::list_products).post(catalogs::create_product),
        )
        .route(
            "/products/{id}",
            get(catalogs::view_product)
                .put(catalogs::edit_product)
                .delete(catalogs::remove_product),
        )
        .route(
            "/customers",
            get(catalogs::list_customers).post(catalogs::create_customer),
        )
        .route(
            "/customers/{id}",
            get(catalogs::view_customer)
                .put(catalogs::edit_customer)
                .delete(catalogs::remove_customer),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ))
        .with_state(state);
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(protected_routes)
}
