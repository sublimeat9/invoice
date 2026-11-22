//! Servidor HTTP. Configuración por variables de entorno (solo lo mínimo para arrancar):
//!
//!   API_TOKEN        token Bearer que exige la API (≥ 16 caracteres)
//!   ENCRYPTION_KEY   clave con la que se cifra el certificado en la base (≥ 16 caracteres)
//!   DB_PATH          archivo SQLite (por defecto facturas.db)
//!   BIND             dirección de escucha (por defecto 127.0.0.1:3000)
//!
//! Los datos del emisor, el ambiente y el certificado .p12 se configuran desde la interfaz
//! (o la API: `PUT /config/issuer`, `PUT /config/certificate`).

use invoice_rust::{
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

#[tokio::main]
async fn main() {
    let token = required_env("API_TOKEN", 16);
    let secret = required_env("ENCRYPTION_KEY", 16);
    let db = std::env::var("DB_PATH").unwrap_or_else(|_| "facturas.db".into());
    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());

    let service = Service::new(Store::open(&db).expect("abrir base de datos"), &secret);
    let state = Arc::new(AppState { service, token });

    // Reintento automático de comprobantes PENDIENTES cada 5 minutos.
    let bg = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            if let Ok(v) = bg.service.retry_pending().await {
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
