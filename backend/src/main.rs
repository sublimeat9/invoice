//! Servidor HTTP. Configuración por variables de entorno (solo lo mínimo para arrancar):
//!
//!   ENCRYPTION_KEY   clave con la que se cifra el certificado en la base (≥ 16 caracteres)
//!   DATABASE_URL     Postgres (p. ej. Supabase); si no está, se usa SQLite
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
    api::{router, AppState},
    boot::service_from_env,
};
use std::{sync::Arc, time::Duration};

#[tokio::main]
async fn main() {
    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let service = service_from_env().await.unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
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
