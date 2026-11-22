//! CRUD de productos y clientes.

use super::{
    errors::{error, service_error},
    AppState,
};
use crate::accounts::User;
use crate::catalog::{CatalogError, Customer, Product};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;

pub(super) fn catalog_error(e: CatalogError) -> Response {
    match e {
        CatalogError::Duplicate => error(StatusCode::CONFLICT, &e.to_string()),
        CatalogError::NotFound => error(StatusCode::NOT_FOUND, &e.to_string()),
        CatalogError::Store(e) => service_error(e.into()),
    }
}

#[derive(Deserialize, Default)]
pub struct CatalogQuery {
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub page: Option<u32>,
}

pub(super) fn optional(o: Option<String>) -> Option<String> {
    o.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub(super) fn validate_product(mut p: Product) -> Result<Product, String> {
    p.main_code = p.main_code.trim().into();
    p.name = p.name.trim().into();
    p.aux_code = optional(p.aux_code);
    let bad = |m: &str| Err(m.to_string());
    if p.main_code.is_empty() || p.main_code.chars().count() > 25 {
        return bad("el código principal debe tener entre 1 y 25 caracteres");
    }
    if p.aux_code.as_ref().is_some_and(|a| a.chars().count() > 25) {
        return bad("el código auxiliar admite máximo 25 caracteres");
    }
    if p.name.is_empty() || p.name.chars().count() > 300 {
        return bad("el nombre debe tener entre 1 y 300 caracteres");
    }
    if !(0..=99_999_999_999).contains(&p.price_cents) {
        return bad("precio fuera de rango");
    }
    if !["15", "5", "0", "not_subject", "exempt"].contains(&p.vat.as_str()) {
        return bad("IVA inválido");
    }
    Ok(p)
}

pub(super) fn validate_customer(mut c: Customer) -> Result<Customer, String> {
    c.id_number = c.id_number.trim().into();
    c.business_name = c.business_name.trim().into();
    c.address = optional(c.address);
    c.phone = optional(c.phone);
    c.email = optional(c.email);
    let bad = |m: &str| Err(m.to_string());
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    match c.id_type.as_str() {
        "05" if !digits(&c.id_number, 10) => return bad("la cédula debe tener 10 dígitos"),
        "04" if !digits(&c.id_number, 13) => return bad("el RUC debe tener 13 dígitos"),
        "06" | "08" if c.id_number.is_empty() || c.id_number.len() > 20 => {
            return bad("identificación inválida (máximo 20 caracteres)")
        }
        "04" | "05" | "06" | "08" => {}
        _ => return bad("tipo de identificación inválido"),
    }
    if c.business_name.is_empty() || c.business_name.chars().count() > 300 {
        return bad("la razón social debe tener entre 1 y 300 caracteres");
    }
    if let Some(e) = &c.email {
        let ok = e.len() <= 100
            && e.split_once('@')
                .is_some_and(|(u, d)| !u.is_empty() && d.contains('.') && !e.contains(' '));
        if !ok {
            return bad("correo electrónico inválido");
        }
    }
    Ok(c)
}

macro_rules! catalog {
    ($Ty:ty, $validate:ident, $list:ident, $create:ident, $view:ident, $edit:ident, $remove:ident,
     $search:ident, $insert:ident, $get_record:ident, $update:ident, $delete:ident) => {
        pub(super) async fn $list(State(st): State<Arc<AppState>>, Extension(me): Extension<User>, Query(q): Query<CatalogQuery>) -> Response {
            match st.service.store.$search(me.id, q.q.as_deref(), q.limit.unwrap_or(50), q.page.unwrap_or(1)).await {
                Ok((items, total)) => Json(serde_json::json!({ "total": total, "items": items })).into_response(),
                Err(e) => catalog_error(e),
            }
        }
        pub(super) async fn $create(State(st): State<Arc<AppState>>, Extension(me): Extension<User>, Json(x): Json<$Ty>) -> Response {
            let x = match $validate(x) { Ok(x) => x, Err(m) => return error(StatusCode::BAD_REQUEST, &m) };
            match async { let id = st.service.store.$insert(me.id, &x).await?; st.service.store.$get_record(me.id, id).await }.await {
                Ok(x) => (StatusCode::CREATED, Json(x)).into_response(),
                Err(e) => catalog_error(e),
            }
        }
        pub(super) async fn $view(State(st): State<Arc<AppState>>, Extension(me): Extension<User>, Path(id): Path<i64>) -> Response {
            match st.service.store.$get_record(me.id, id).await { Ok(x) => Json(x).into_response(), Err(e) => catalog_error(e) }
        }
        pub(super) async fn $edit(State(st): State<Arc<AppState>>, Extension(me): Extension<User>, Path(id): Path<i64>, Json(x): Json<$Ty>) -> Response {
            let x = match $validate(x) { Ok(x) => x, Err(m) => return error(StatusCode::BAD_REQUEST, &m) };
            match async { st.service.store.$update(me.id, id, &x).await?; st.service.store.$get_record(me.id, id).await }.await {
                Ok(x) => Json(x).into_response(),
                Err(e) => catalog_error(e),
            }
        }
        pub(super) async fn $remove(State(st): State<Arc<AppState>>, Extension(me): Extension<User>, Path(id): Path<i64>) -> Response {
            match st.service.store.$delete(me.id, id).await { Ok(()) => StatusCode::NO_CONTENT.into_response(), Err(e) => catalog_error(e) }
        }
    };
}

catalog!(
    Product,
    validate_product,
    list_products,
    create_product,
    view_product,
    edit_product,
    remove_product,
    search_products,
    create_product,
    get_product,
    update_product,
    delete_product
);
catalog!(
    Customer,
    validate_customer,
    list_customers,
    create_customer,
    view_customer,
    edit_customer,
    remove_customer,
    search_customers,
    create_customer,
    get_customer,
    update_customer,
    delete_customer
);
