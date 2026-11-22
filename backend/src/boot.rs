//! Arranque común del servidor y de la función de Vercel: lee el entorno y arma el `Service`.

use crate::{accounts::Role, db::Db, pg::PgStore, service::Service, store::Store};

fn required_env(k: &str, min: usize) -> Result<String, String> {
    let v = std::env::var(k).unwrap_or_default();
    if v.len() < min {
        return Err(format!(
            "{k} es obligatoria y debe tener al menos {min} caracteres"
        ));
    }
    Ok(v)
}

/// Crea el primer administrador desde el entorno, solo si todavía no hay usuarios.
async fn bootstrap_admin(store: &dyn Db) -> Result<(), String> {
    let (Ok(user), Ok(password)) = (std::env::var("ADMIN_USER"), std::env::var("ADMIN_PASSWORD"))
    else {
        return Ok(());
    };
    if store.count_users().await.map_err(|e| e.to_string())? > 0 {
        return Ok(());
    }
    store
        .create_user(&user, &password, Role::Admin)
        .await
        .map(|u| println!("administrador '{}' creado", u.username))
        .map_err(|e| format!("no se pudo crear el administrador: {e}"))
}

/// Variables: `ENCRYPTION_KEY` (obligatoria); `DATABASE_URL` (Postgres/Supabase) o, si no hay,
/// `DB_PATH` (archivo SQLite); `CATASTRO_URL` y `ADMIN_USER`/`ADMIN_PASSWORD` (opcionales).
pub async fn service_from_env() -> Result<Service, String> {
    let secret = required_env("ENCRYPTION_KEY", 16)?;
    let mut service = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => {
            let store = PgStore::connect(url.trim())
                .await
                .map_err(|e| format!("no se pudo conectar a Postgres: {e}"))?;
            Service::new(store, &secret)
        }
        _ => {
            let path = std::env::var("DB_PATH").unwrap_or_else(|_| "facturas.db".into());
            let store = Store::open(&path).map_err(|e| format!("no se pudo abrir la base: {e}"))?;
            Service::new(store, &secret)
        }
    };
    if let Ok(base) = std::env::var("CATASTRO_URL") {
        service = service.with_catastro_base(base);
    }
    bootstrap_admin(service.store.as_ref()).await?;
    Ok(service)
}
