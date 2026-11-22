//! Servidor HTTP. Configuración por variables de entorno:
//!
//!   P12_PATH, P12_PASSWORD, API_TOKEN            (obligatorias)
//!   EMISOR_RUC, EMISOR_RAZON_SOCIAL, EMISOR_DIR_MATRIZ y opcionales EMISOR_* (ver `Emisor::desde_env`)
//!   AMBIENTE=pruebas|produccion (por defecto pruebas), DB_PATH (facturas.db), BIND (127.0.0.1:3000)

use invoice_rust::{
    api::{router, AppState},
    clave_acceso::Ambiente,
    factura::Emisor,
    firma::Firmador,
    servicio::Servicio,
    sri::SriClient,
    store::Store,
};
use std::{sync::Arc, time::Duration};

fn requerida(k: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| {
        eprintln!("falta la variable de entorno {k}");
        std::process::exit(2);
    })
}

#[tokio::main]
async fn main() {
    let ambiente = match std::env::var("AMBIENTE").as_deref() {
        Ok("produccion") => Ambiente::Produccion,
        Ok("pruebas") | Err(_) => Ambiente::Pruebas,
        Ok(otro) => {
            eprintln!("AMBIENTE debe ser pruebas o produccion, no {otro:?}");
            std::process::exit(2);
        }
    };
    let token = requerida("API_TOKEN");
    if token.len() < 16 {
        eprintln!("API_TOKEN debe tener al menos 16 caracteres");
        std::process::exit(2);
    }
    let p12 = std::fs::read(requerida("P12_PATH")).expect("no se pudo leer P12_PATH");
    let emisor = Emisor::desde_env().unwrap_or_else(|e| {
        eprintln!("configuración del emisor: {e}");
        std::process::exit(2);
    });
    let db = std::env::var("DB_PATH").unwrap_or_else(|_| "facturas.db".into());
    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());

    let servicio = Servicio {
        store: Store::abrir(&db).expect("abrir base de datos"),
        firmador: Firmador::desde_p12(&p12, &requerida("P12_PASSWORD")).expect("abrir .p12"),
        sri: SriClient::new(ambiente),
        ambiente,
        emisor,
    };
    let state = Arc::new(AppState { servicio, token });

    // Reintento automático de comprobantes PENDIENTES cada 5 minutos.
    let bg = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            if let Ok(v) = bg.servicio.reintentar_pendientes().await {
                for (clave, r) in v {
                    match r {
                        Ok(reg) => println!("reintento {clave}: {}", reg.estado),
                        Err(e) => eprintln!("reintento {clave}: {e}"),
                    }
                }
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&bind).await.expect("no se pudo abrir BIND");
    println!("escuchando en http://{bind} (ambiente {ambiente:?})");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
}
