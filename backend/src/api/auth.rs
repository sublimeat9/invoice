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
use std::sync::Arc;

const MAX_FAILURES: u32 = 5;
/// Sign-ups allowed per lockout window, across all clients.
const MAX_REGISTRATIONS: u32 = 20;

/// The session token of the current request (extension set by [`authenticate`]).
#[derive(Clone)]
pub(super) struct SessionToken(String);

/// Failed attempts are counted in the database, so the limit holds across server instances.
const LOCKOUT_SECONDS: i64 = 15 * 60;

async fn throttled(st: &AppState, key: &str, max: u32) -> bool {
    st.service
        .store
        .throttle_blocked(key, max, LOCKOUT_SECONDS)
        .await
        .unwrap_or(false)
}

async fn throttle_hit(st: &AppState, key: &str) {
    let _ = st.service.store.throttle_hit(key).await;
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
    match st.service.store.session_user(&token).await {
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

async fn session_response(st: &AppState, user: User) -> Response {
    match st.service.store.create_session(user.id).await {
        Ok((token, expires_at)) => {
            Json(serde_json::json!({ "token": token, "expires_at": expires_at, "user": user }))
                .into_response()
        }
        Err(e) => account_error(e),
    }
}

/// Tells the interface whether it must show the first-run setup instead of the login.
pub(super) async fn status(State(st): State<Arc<AppState>>) -> Response {
    match st.service.store.count_users().await {
        Ok(n) => Json(serde_json::json!({ "setup_required": n == 0 })).into_response(),
        Err(e) => account_error(e),
    }
}

/// Creates the first administrator. Only works while there are no users at all.
pub(super) async fn setup(State(st): State<Arc<AppState>>, Json(c): Json<Credentials>) -> Response {
    match st.service.store.count_users().await {
        Ok(0) => {}
        Ok(_) => return error(StatusCode::CONFLICT, "la configuración inicial ya se hizo"),
        Err(e) => return account_error(e),
    }
    match st
        .service
        .store
        .create_user(&c.username, &c.password, Role::Admin)
        .await
    {
        Ok(user) => session_response(&st, user).await,
        Err(e) => account_error(e),
    }
}

#[derive(Deserialize)]
pub(super) struct Registration {
    ruc: String,
    password: String,
}

/// Public sign-up: the RUC is the username (the first account becomes the administrator). The taxpayer data comes from the SRI, so the
/// account starts with the real issuer details; it only needs the signature (.p12) afterwards.
pub(super) async fn register(
    State(st): State<Arc<AppState>>,
    Json(c): Json<Registration>,
) -> Response {
    let ruc = c.ruc.trim().to_string();
    // The very first account (a desktop install, say) becomes the administrator.
    let role = match st.service.store.count_users().await {
        Ok(0) => Role::Admin,
        Ok(_) => Role::Operator,
        Err(e) => return account_error(e),
    };
    if let Err(e) = accounts::validate_password(&c.password) {
        return account_error(e);
    }
    if !crate::catastro::valid_shape(&ruc) {
        return error(StatusCode::BAD_REQUEST, "el RUC debe tener 13 dígitos");
    }
    // Global cap so the endpoint cannot be used to flood the user table or the SRI.
    const KEY: &str = "\0register";
    if throttled(&st, KEY, MAX_REGISTRATIONS).await {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "demasiados registros; intente más tarde",
        );
    }
    throttle_hit(&st, KEY).await;
    if matches!(st.service.store.find_login(&ruc).await, Ok(Some(_))) {
        return error(StatusCode::CONFLICT, "ya existe una cuenta con ese RUC");
    }
    let issuer = match st.service.catastro().lookup(&ruc).await {
        Ok(i) => i,
        Err(e @ crate::catastro::CatastroError::Unavailable(_)) => {
            return error(StatusCode::SERVICE_UNAVAILABLE, &e.to_string())
        }
        Err(e) => return error(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    let user = match st.service.store.create_user(&ruc, &c.password, role).await {
        Ok(u) => u,
        Err(e) => return account_error(e),
    };
    if let Err(e) = st
        .service
        .store
        .save_issuer(user.id, &issuer, crate::access_key::Environment::Testing)
        .await
    {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    session_response(&st, user).await
}

pub(super) async fn login(State(st): State<Arc<AppState>>, Json(c): Json<Credentials>) -> Response {
    let key = c.username.trim().to_lowercase();
    if throttled(&st, &key, MAX_FAILURES).await {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "demasiados intentos fallidos; espere 15 minutos",
        );
    }
    let found = match st.service.store.find_login(&c.username).await {
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
            let _ = st.service.store.throttle_clear(&key).await;
            session_response(&st, user).await
        }
        _ => {
            throttle_hit(&st, &key).await;
            error(StatusCode::UNAUTHORIZED, "usuario o contraseña incorrectos")
        }
    }
}

pub(super) async fn logout(
    State(st): State<Arc<AppState>>,
    Extension(token): Extension<SessionToken>,
) -> Response {
    match st.service.store.delete_session(&token.0).await {
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
    let hash = match st.service.store.password_hash_of(user.id).await {
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
    let store = &st.service.store;
    let result = match store.set_password(user.id, &c.new_password).await {
        Ok(()) => store.delete_other_sessions(user.id, &token.0).await,
        Err(e) => Err(e),
    };
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => account_error(e),
    }
}

/// Reintento de comprobantes pendientes para despliegues sin proceso permanente (Vercel Cron).
/// Exige `Authorization: Bearer $CRON_SECRET`; sin `CRON_SECRET` configurado, la ruta no existe.
pub(super) async fn cron_retry(State(st): State<Arc<AppState>>, req: Request) -> Response {
    let secret = std::env::var("CRON_SECRET").unwrap_or_default();
    if secret.len() < 16 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let given = bearer(&req).unwrap_or_default();
    // Constant-time comparison: do not leak the secret through response timing.
    let equal = given.len() == secret.len()
        && given
            .bytes()
            .zip(secret.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if !equal {
        return error(StatusCode::UNAUTHORIZED, "no autorizado");
    }
    match st.service.retry_pending(None).await {
        Ok(v) => Json(serde_json::json!({
            "retried": v.len(),
            "results": v.iter().map(|(k, r)| match r {
                Ok(rec) => serde_json::json!({ "access_key": k, "status": rec.status }),
                Err(e) => serde_json::json!({ "access_key": k, "error": e.to_string() }),
            }).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => super::errors::service_error(e),
    }
}
