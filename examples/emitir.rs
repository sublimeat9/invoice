//! Emite una factura de prueba de punta a punta: XML -> firma -> recepción -> autorización.
//!
//!   P12_PASSWORD=... EMISOR_RUC=... EMISOR_RAZON_SOCIAL=... EMISOR_DIR_MATRIZ=... \
//!     cargo run --example emitir -- <cert.p12> [secuencial] [produccion]
//!
//! Variables opcionales: EMISOR_NOMBRE_COMERCIAL, EMISOR_DIR_ESTABLECIMIENTO,
//! EMISOR_OBLIGADO_CONTABILIDAD (SI/NO), EMISOR_ESTAB, EMISOR_PTO_EMI.
//!
//! Por defecto usa el ambiente de PRUEBAS (celcer.sri.gob.ec).
use invoice_rust::{clave_acceso::Ambiente, factura::*, firma::Firmador, sri::SriClient};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let p12 = std::fs::read(a.next().expect("ruta .p12")).expect("leer .p12");
    let secuencial: u32 = match a.next() {
        None => 1,
        Some(s) => match s.parse::<u32>() {
            Ok(n) if n >= 1 && n <= 999_999_999 => n,
            _ => {
                eprintln!("secuencial inválido {s:?}: debe ser un número de 1 a 999999999.");
                eprintln!("(el RUC ya no es argumento: defínalo en EMISOR_RUC)");
                std::process::exit(2);
            }
        },
    };
    let ambiente = if a.next().as_deref() == Some("produccion") { Ambiente::Produccion } else { Ambiente::Pruebas };
    let pass = std::env::var("P12_PASSWORD").expect("defina P12_PASSWORD");

    let emisor = Emisor::desde_env().unwrap_or_else(|e| {
        eprintln!("configuración del emisor: {e}");
        std::process::exit(2);
    });

    let hoy = String::from_utf8(
        std::process::Command::new("date").arg("+%d%m%Y").output().unwrap().stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let factura = Factura {
        ambiente,
        emisor,
        comprador: Comprador::consumidor_final(),
        fecha_emision: hoy,
        secuencial,
        codigo_numerico: invoice_rust::clave_acceso::codigo_numerico_aleatorio(),
        lineas: vec![Linea {
            codigo_principal: "S/C".into(),
            descripcion: "ASISTENCIA".into(),
            cantidad: 1,
            precio_unitario: 1500,
            descuento: 0,
            iva: Iva::Quince,
        }],
        forma_pago: "01".into(),
        info_adicional: vec![("Email".into(), "cliente@example.com".into())],
    };

    let (xml, clave) = factura.to_xml().expect("generar XML");
    println!("clave de acceso: {clave}");
    let firmado = Firmador::desde_p12(&p12, &pass)
        .expect("abrir .p12")
        .firmar(&xml)
        .expect("firmar");
    std::fs::write(format!("{clave}.xml"), &firmado).expect("guardar XML firmado");

    let sri = SriClient::new(ambiente);
    let rec = sri.recepcion(firmado.as_bytes()).await.expect("recepción");
    println!("recepción: {}", rec.estado);
    for m in &rec.mensajes {
        println!("  [{}] {} {}: {} {}", m.tipo, m.identificador, "", m.mensaje, m.informacion_adicional.as_deref().unwrap_or(""));
    }
    if !rec.recibida() {
        std::process::exit(1);
    }

    match sri.esperar_autorizacion(&clave, Duration::from_secs(3), 10).await {
        Ok(au) => {
            println!("autorización: {}", au.estado);
            println!("  número: {:?}\n  fecha: {:?}", au.numero_autorizacion, au.fecha_autorizacion);
            for m in &au.mensajes {
                println!("  [{}] {}: {} {}", m.tipo, m.identificador, m.mensaje, m.informacion_adicional.as_deref().unwrap_or(""));
            }
            if au.estado != "AUTORIZADO" {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("error esperando autorización: {e}");
            std::process::exit(1);
        }
    }
}
