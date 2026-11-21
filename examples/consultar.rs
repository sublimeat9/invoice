//! cargo run --example consultar -- <clave_acceso_49_digitos> [produccion]
use invoice_rust::{clave_acceso::{validar, Ambiente}, sri::SriClient};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let clave = args.next().expect("falta la clave de acceso");
    let amb = if args.next().as_deref() == Some("produccion") { Ambiente::Produccion } else { Ambiente::Pruebas };
    println!("clave válida (módulo 11): {}", validar(&clave));
    let r = SriClient::new(amb).autorizacion(&clave).await;
    println!("{r:#?}");
}
