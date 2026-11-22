//! API HTTP (JSON) sobre `Service`. Un módulo por recurso:
//!
//! | módulo     | rutas                                                                    |
//! |------------|--------------------------------------------------------------------------|
//! | `auth`     | `/auth/status`, `/auth/setup`, `/auth/register`, `/auth/login`, `/auth/logout`, `/auth/me`, `/auth/password` |
//! | `users`    | `/users` (solo administradores)                                          |
//! | `invoices` | `POST /invoices`, `GET /invoices/{access_key}`, `POST /invoices/retry`   |
//! | `search`   | `GET /invoices` (filtros) y `GET /invoices/report.csv`                   |
//! | `config`   | `GET /config` (todos); `PUT /config/issuer` y `/config/certificate` (admin) |
//! | `catalogs` | `/products` y `/customers` (CRUD)                                        |
//! | `dispatch` | `dispatch`: invoca el router dentro del proceso (app de escritorio)      |
//!
//! Todo exige `Authorization: Bearer <token de sesión>` salvo `/health` y las rutas de
//! `/auth` para entrar. Los montos viajan en centavos enteros.

mod auth;
mod catalogs;
mod config;
mod dispatch;
mod errors;
mod invoices;
mod search;
mod users;

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
}

impl AppState {
    pub fn new(service: Service) -> Self {
        Self { service }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    let login_required = || middleware::from_fn_with_state(state.clone(), auth::authenticate);

    // Any signed-in user.
    let signed_in = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/password", put(auth::change_password))
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
        .layer(login_required());

    // Administrators only: managing users. The issuer and certificate belong to each account.
    let admin_only = Router::new()
        .route("/users", get(users::list_users).post(users::create_user))
        .route("/users/{id}", put(users::update_user))
        .layer(middleware::from_fn(auth::require_admin))
        .layer(login_required());

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/internal/retry", get(auth::cron_retry))
        .route("/auth/status", get(auth::status))
        .route("/auth/setup", post(auth::setup))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .merge(signed_in)
        .merge(admin_only)
        .with_state(state)
}
