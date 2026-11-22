//! Servidor HTTP. Configuración por variables de entorno (solo lo mínimo para arrancar):
//!
//!   ENCRYPTION_KEY   clave con la que se cifra el certificado en la base (≥ 16 caracteres)
//!   DB_PATH          archivo SQLite (por defecto facturas.db)
//!   BIND             dirección de escucha (por defecto 127.0.0.1:3000)
//!   CATASTRO_URL     (opcional) base del servicio de consulta de RUC del SRI, para pruebas
//!   ADMIN_USER / ADMIN_PASSWORD   (opcionales) crean el primer administrador al arrancar si
//!                    todavía no hay usuarios; sin ellas, el primero que abra la interfaz hace
//!                    la configuración inicial. En un servidor expuesto, defínalas.
//!
//! Los datos del emisor, el ambiente y el certificado .p12 se configuran desde la interfaz
//! (o la API: `PUT /config/issuer`, `PUT /config/certificate`).

use invoice_rust::{
    accounts::Role,
    api::{router, AppState},
    service::Service,
    store::Store,
};
use std::{sync::Arc, time::Duration};

fn required_env(k: &str, min: usize) -> String {
    let v = std::env::var(k).unwrap_or_default();
    if v.len() < min {
        eprintln!("{k} es obligatoria y debe tener al menos {min} caracteres");
        std::process::exit(2);
    }
    v
}

/// Creates the first administrator from the environment, only if there are no users yet.
fn bootstrap_admin(store: &Store) {
    let (Ok(user), Ok(password)) = (std::env::var("ADMIN_USER"), std::env::var("ADMIN_PASSWORD"))
    else {
        return;
    };
    if store.count_users().unwrap_or(1) > 0 {
        return;
    }
    match store.create_user(&user, &password, Role::Admin) {
        Ok(u) => println!("administrador '{}' creado", u.username),
        Err(e) => {
            eprintln!("no se pudo crear el administrador: {e}");
            std::process::exit(2);
        }
    }
}

#[tokio::main]
async fn main() {
    let secret = required_env("ENCRYPTION_KEY", 16);
    let db = std::env::var("DB_PATH").unwrap_or_else(|_| "facturas.db".into());
    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());

    let mut service = Service::new(Store::open(&db).expect("abrir base de datos"), &secret);
    if let Ok(base) = std::env::var("CATASTRO_URL") {
        service = service.with_catastro_base(base);
    }
    bootstrap_admin(&service.store);
    let state = Arc::new(AppState::new(service));

    // Reintento automático de comprobantes PENDIENTES cada 5 minutos.
    let bg = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            if let Ok(v) = bg.service.retry_pending(None).await {
                for (access_key, r) in v {
                    match r {
                        Ok(record) => println!("reintento {access_key}: {}", record.status),
                        Err(e) => eprintln!("reintento {access_key}: {e}"),
                    }
                }
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("no se pudo abrir BIND");
    println!("escuchando en http://{bind}");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
}
