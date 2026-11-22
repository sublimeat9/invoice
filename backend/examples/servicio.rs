//! Emite una factura usando la capa de servicio (secuencial automático + SQLite).
//!
//!   P12_PASSWORD=... EMISOR_RUC=... EMISOR_RAZON_SOCIAL=... EMISOR_DIR_MATRIZ=... \
//!     cargo run --example servicio -- <cert.p12> [base.db] [produccion] [reintentar] [ultimo=N]
//!
//! `ultimo=N` indica el último secuencial ya usado (el siguiente será N+1); nunca baja
//! un contador existente.
use invoice_rust::{
    clave_acceso::Ambiente,
    factura::*,
    firma::Firmador,
    servicio::{Servicio, SolicitudFactura},
    sri::SriClient,
    store::Store,
};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let p12 = std::fs::read(args.first().expect("ruta .p12")).expect("leer .p12");
    let db = args.get(1).filter(|a| a.ends_with(".db")).cloned().unwrap_or("facturas.db".into());
    let ambiente = if args.iter().any(|a| a == "produccion") { Ambiente::Produccion } else { Ambiente::Pruebas };
    let pass = std::env::var("P12_PASSWORD").expect("defina P12_PASSWORD");
    let emisor = Emisor::desde_env().unwrap_or_else(|e| {
        eprintln!("configuración del emisor: {e}");
        std::process::exit(2);
    });

    let srv = Servicio {
        store: Store::abrir(&db).expect("abrir base de datos"),
        firmador: Firmador::desde_p12(&p12, &pass).expect("abrir .p12"),
        sri: SriClient::new(ambiente),
        ambiente,
        emisor,
    };

    if let Some(n) = args.iter().find_map(|a| a.strip_prefix("ultimo=")) {
        let n: u32 = n.parse().unwrap_or_else(|_| {
            eprintln!("ultimo=N: N debe ser un número entero");
            std::process::exit(2);
        });
        srv.store
            .fijar_secuencial(ambiente, &srv.emisor.estab, &srv.emisor.pto_emi, n)
            .expect("fijar secuencial");
        println!("secuencial {}-{}: último usado >= {n}", srv.emisor.estab, srv.emisor.pto_emi);
    }

    if args.iter().any(|a| a == "reintentar") {
        for (clave, r) in srv.reintentar_pendientes().await.expect("pendientes") {
            match r {
                Ok(reg) => println!("{clave}: {}", reg.estado),
                Err(e) => println!("{clave}: sigue pendiente ({e})"),
            }
        }
        return;
    }

    let r = srv
        .emitir(SolicitudFactura {
            comprador: Comprador::consumidor_final(),
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
        })
        .await;
    match r {
        Ok(reg) => {
            println!("secuencial {} -> {}", reg.secuencial, reg.estado);
            println!("clave: {}", reg.clave_acceso);
            if let Some(m) = reg.mensajes {
                println!("{m}");
            }
        }
        Err(e) => {
            eprintln!("error: {e}\n(si ya se firmó, quedó PENDIENTE: use `reintentar`)");
            std::process::exit(1);
        }
    }
}
