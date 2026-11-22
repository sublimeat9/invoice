//! Login, sessions and the guards that protect the other routes.

use super::{
    errors::{account_error, error},
    AppState,
};
use crate::accounts::{self, Role, User};
use axum::{
    extract::{Extension, Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_FAILURES: u32 = 5;
/// Sign-ups allowed per lockout window, across all clients.
const MAX_REGISTRATIONS: u32 = 20;
const LOCKOUT: Duration = Duration::from_secs(15 * 60);

/// The session token of the current request (extension set by [`authenticate`]).
#[derive(Clone)]
pub(super) struct SessionToken(String);

/// Counts failed logins per username to slow down password guessing.
#[derive(Default)]
pub struct LoginThrottle(Mutex<HashMap<String, (u32, Instant)>>);

impl LoginThrottle {
    fn blocked(&self, user: &str) -> bool {
        self.blocked_at(user, MAX_FAILURES)
    }

    fn blocked_at(&self, key: &str, max: u32) -> bool {
        let mut map = self.0.lock().unwrap();
        match map.get(key) {
            Some((_, since)) if since.elapsed() > LOCKOUT => {
                map.remove(key);
                false
            }
            Some((n, _)) => *n >= max,
            None => false,
        }
    }

    fn failed(&self, user: &str) {
        let mut map = self.0.lock().unwrap();
        let entry = map.entry(user.to_string()).or_insert((0, Instant::now()));
        entry.0 += 1;
    }

    fn succeeded(&self, user: &str) {
        self.0.lock().unwrap().remove(user);
    }
}

fn bearer(req: &Request) -> Option<String> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
}

/// Requires a valid session; exposes the user and the token to the handlers.
pub(super) async fn authenticate(
    State(st): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let Some(token) = bearer(&req) else {
        return error(StatusCode::UNAUTHORIZED, "inicie sesión");
    };
    match st.service.store.session_user(&token) {
        Ok(Some(user)) => {
            req.extensions_mut().insert(user);
            req.extensions_mut().insert(SessionToken(token));
            next.run(req).await
        }
        Ok(None) => error(
            StatusCode::UNAUTHORIZED,
            "la sesión venció, inicie sesión de nuevo",
        ),
        Err(e) => account_error(e),
    }
}

/// Must run after [`authenticate`].
pub(super) async fn require_admin(req: Request, next: Next) -> Response {
    match req.extensions().get::<User>() {
        Some(u) if u.role == Role::Admin => next.run(req).await,
        _ => error(
            StatusCode::FORBIDDEN,
            "solo un administrador puede hacer esto",
        ),
    }
}

#[derive(Deserialize)]
pub(super) struct Credentials {
    username: String,
    password: String,
}

fn session_response(st: &AppState, user: User) -> Response {
    match st.service.store.create_session(user.id) {
        Ok((token, expires_at)) => {
            Json(serde_json::json!({ "token": token, "expires_at": expires_at, "user": user }))
                .into_response()
        }
        Err(e) => account_error(e),
    }
}

/// Tells the interface whether it must show the first-run setup instead of the login.
pub(super) async fn status(State(st): State<Arc<AppState>>) -> Response {
    match st.service.store.count_users() {
        Ok(n) => Json(serde_json::json!({ "setup_required": n == 0 })).into_response(),
        Err(e) => account_error(e),
    }
}

/// Creates the first administrator. Only works while there are no users at all.
pub(super) async fn setup(State(st): State<Arc<AppState>>, Json(c): Json<Credentials>) -> Response {
    match st.service.store.count_users() {
        Ok(0) => {}
        Ok(_) => return error(StatusCode::CONFLICT, "la configuración inicial ya se hizo"),
        Err(e) => return account_error(e),
    }
    match st
        .service
        .store
        .create_user(&c.username, &c.password, Role::Admin)
    {
        Ok(user) => session_response(&st, user),
        Err(e) => account_error(e),
    }
}

/// Public sign-up: anyone can create an operator account. The accounts are active right away.
pub(super) async fn register(
    State(st): State<Arc<AppState>>,
    Json(c): Json<Credentials>,
) -> Response {
    match st.service.store.count_users() {
        Ok(0) => return error(StatusCode::CONFLICT, "primero cree el administrador"),
        Ok(_) => {}
        Err(e) => return account_error(e),
    }
    // Global cap so the endpoint cannot be used to flood the user table.
    const KEY: &str = "\0register";
    if st.login_throttle.blocked_at(KEY, MAX_REGISTRATIONS) {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "demasiados registros; intente más tarde",
        );
    }
    match st
        .service
        .store
        .create_user(&c.username, &c.password, Role::Operator)
    {
        Ok(user) => {
            st.login_throttle.failed(KEY);
            session_response(&st, user)
        }
        Err(e) => account_error(e),
    }
}

pub(super) async fn login(State(st): State<Arc<AppState>>, Json(c): Json<Credentials>) -> Response {
    let key = c.username.trim().to_lowercase();
    if st.login_throttle.blocked(&key) {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "demasiados intentos fallidos; espere 15 minutos",
        );
    }
    let found = match st.service.store.find_login(&c.username) {
        Ok(f) => f,
        Err(e) => return account_error(e),
    };
    // Argon2 is CPU heavy: keep it off the async worker threads.
    let password = c.password.clone();
    let (user, valid) = tokio::task::spawn_blocking(move || match found {
        Some((user, hash)) => {
            let ok = accounts::verify_password(&password, &hash) && user.active;
            (Some(user), ok)
        }
        None => {
            accounts::verify_dummy(&password);
            (None, false)
        }
    })
    .await
    .unwrap_or((None, false));
    match (user, valid) {
        (Some(user), true) => {
            st.login_throttle.succeeded(&key);
            session_response(&st, user)
        }
        _ => {
            st.login_throttle.failed(&key);
            error(StatusCode::UNAUTHORIZED, "usuario o contraseña incorrectos")
        }
    }
}

pub(super) async fn logout(
    State(st): State<Arc<AppState>>,
    Extension(token): Extension<SessionToken>,
) -> Response {
    match st.service.store.delete_session(&token.0) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => account_error(e),
    }
}

pub(super) async fn me(Extension(user): Extension<User>) -> Response {
    Json(user).into_response()
}

#[derive(Deserialize)]
pub(super) struct PasswordChange {
    current_password: String,
    new_password: String,
}

/// Changes the caller's own password and closes their other sessions.
pub(super) async fn change_password(
    State(st): State<Arc<AppState>>,
    Extension(user): Extension<User>,
    Extension(token): Extension<SessionToken>,
    Json(c): Json<PasswordChange>,
) -> Response {
    let hash = match st.service.store.password_hash_of(user.id) {
        Ok(Some(h)) => h,
        Ok(None) => return error(StatusCode::NOT_FOUND, "usuario no encontrado"),
        Err(e) => return account_error(e),
    };
    let current = c.current_password.clone();
    let valid = tokio::task::spawn_blocking(move || accounts::verify_password(&current, &hash))
        .await
        .unwrap_or(false);
    if !valid {
        return error(
            StatusCode::BAD_REQUEST,
            "la contraseña actual no es correcta",
        );
    }
    let result = st
        .service
        .store
        .set_password(user.id, &c.new_password)
        .and_then(|()| st.service.store.delete_other_sessions(user.id, &token.0));
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => account_error(e),
    }
}
