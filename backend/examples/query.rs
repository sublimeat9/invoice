//! cargo run --example consultar -- <clave_acceso_49_digitos> [produccion]
use invoice_rust::{
    access_key::{validate, Environment},
    sri::SriClient,
};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let access_key = args.next().expect("falta la clave de acceso");
    let env_label = if args.next().as_deref() == Some("produccion") {
        Environment::Production
    } else {
        Environment::Testing
    };
    println!("clave válida (módulo 11): {}", validate(&access_key));
    let r = SriClient::new(env_label).authorization(&access_key).await;
    println!("{r:#?}");
}
