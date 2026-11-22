//! User management (administrators only).

use super::{errors::account_error, AppState};
use crate::accounts::Role;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

pub(super) async fn list_users(State(st): State<Arc<AppState>>) -> Response {
    match st.service.store.list_users().await {
        Ok(users) => Json(users).into_response(),
        Err(e) => account_error(e),
    }
}

#[derive(Deserialize)]
pub(super) struct NewUser {
    username: String,
    password: String,
    role: Role,
}

pub(super) async fn create_user(
    State(st): State<Arc<AppState>>,
    Json(u): Json<NewUser>,
) -> Response {
    match st
        .service
        .store
        .create_user(&u.username, &u.password, u.role)
        .await
    {
        Ok(user) => (axum::http::StatusCode::CREATED, Json(user)).into_response(),
        Err(e) => account_error(e),
    }
}

#[derive(Deserialize)]
pub(super) struct UserChanges {
    role: Option<Role>,
    active: Option<bool>,
    /// Sets a new password (the user's open sessions are closed).
    password: Option<String>,
}

pub(super) async fn update_user(
    State(st): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(c): Json<UserChanges>,
) -> Response {
    match st
        .service
        .store
        .update_user(id, c.role, c.active, c.password.as_deref())
        .await
    {
        Ok(user) => Json(user).into_response(),
        Err(e) => account_error(e),
    }
}
