//! Emite una factura de prueba de punta a punta: XML -> firma -> recepción -> autorización.
//!
//!   P12_PASSWORD=... EMISOR_RUC=... EMISOR_RAZON_SOCIAL=... EMISOR_DIR_MATRIZ=... \
//!     cargo run --example emitir -- <cert.p12> [secuencial] [produccion]
//!
//! Variables opcionales: EMISOR_NOMBRE_COMERCIAL, EMISOR_DIR_ESTABLECIMIENTO,
//! EMISOR_OBLIGADO_CONTABILIDAD (SI/NO), EMISOR_ESTAB, EMISOR_PTO_EMI.
//!
//! Por defecto usa el ambiente de PRUEBAS (celcer.sri.gob.ec).
use invoice_rust::{access_key::Environment, invoice::*, signing::CertSigner, sri::SriClient};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let p12 = std::fs::read(a.next().expect("ruta .p12")).expect("leer .p12");
    let sequence: u32 = match a.next() {
        None => 1,
        Some(s) => match s.parse::<u32>() {
            Ok(n) if (1..=999_999_999).contains(&n) => n,
            _ => {
                eprintln!("secuencial inválido {s:?}: debe ser un número de 1 a 999999999.");
                eprintln!("(el RUC ya no es argumento: defínalo en EMISOR_RUC)");
                std::process::exit(2);
            }
        },
    };
    let environment = if a.next().as_deref() == Some("produccion") {
        Environment::Production
    } else {
        Environment::Testing
    };
    let pass = std::env::var("P12_PASSWORD").expect("defina P12_PASSWORD");

    let issuer = Issuer::from_env().unwrap_or_else(|e| {
        eprintln!("configuración del emisor: {e}");
        std::process::exit(2);
    });

    let today = String::from_utf8(
        std::process::Command::new("date")
            .arg("+%d%m%Y")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let invoice = Invoice {
        environment,
        issuer,
        buyer: Buyer::final_consumer(),
        issue_date: today,
        sequence,
        numeric_code: invoice_rust::access_key::random_numeric_code(),
        lines: vec![Line {
            main_code: "S/C".into(),
            description: "ASISTENCIA".into(),
            quantity: 1,
            unit_price: 1500,
            discount: 0,
            vat: Vat::Fifteen,
        }],
        payment_method: "01".into(),
        additional_info: vec![("Email".into(), "cliente@example.com".into())],
    };

    let (xml, access_key) = invoice.to_xml().expect("generar XML");
    println!("clave de acceso: {access_key}");
    let signed = CertSigner::from_p12(&p12, &pass)
        .expect("abrir .p12")
        .sign(&xml)
        .expect("firmar");
    std::fs::write(format!("{access_key}.xml"), &signed).expect("guardar XML firmado");

    let sri = SriClient::new(environment);
    let reception = sri.reception(signed.as_bytes()).await.expect("recepción");
    println!("recepción: {}", reception.status);
    for m in &reception.messages {
        println!(
            "  [{}] {}: {} {}",
            m.kind,
            m.identifier,
            m.message,
            m.additional_information.as_deref().unwrap_or("")
        );
    }
    if !reception.received() {
        std::process::exit(1);
    }

    match sri
        .wait_for_authorization(&access_key, Duration::from_secs(3), 10)
        .await
    {
        Ok(authorization) => {
            println!("autorización: {}", authorization.status);
            println!(
                "  número: {:?}\n  fecha: {:?}",
                authorization.authorization_number, authorization.authorization_date
            );
            for m in &authorization.messages {
                println!(
                    "  [{}] {}: {} {}",
                    m.kind,
                    m.identifier,
                    m.message,
                    m.additional_information.as_deref().unwrap_or("")
                );
            }
            if authorization.status != "AUTORIZADO" {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("error esperando autorización: {e}");
            std::process::exit(1);
        }
    }
}
