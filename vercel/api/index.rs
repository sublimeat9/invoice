//! Backend como función de Vercel. Todas las rutas entran por aquí (ver `vercel.json`).
//!
//! Variables de entorno (en el proyecto de Vercel, nunca en el repositorio):
//!   DATABASE_URL    cadena de conexión de Supabase (pooler, puerto 6543 o 5432)
//!   ENCRYPTION_KEY  clave que cifra los certificados .p12 en la base (≥ 16 caracteres)
//!   CRON_SECRET     secreto del cron de reintentos (≥ 16 caracteres)
//!   ADMIN_USER / ADMIN_PASSWORD   (opcionales) primer administrador

use invoice_rust::{
    api::{router, AppState},
    boot::service_from_env,
};
use std::sync::Arc;
use tower::ServiceBuilder;
use vercel_runtime::{axum::VercelLayer, run, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let service = service_from_env().await?;
    let app = ServiceBuilder::new()
        .layer(VercelLayer::new())
        .service(router(Arc::new(AppState::new(service))));
    run(app).await
}
