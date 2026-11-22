//! P12_PASSWORD=... cargo run --example firmar -- <cert.p12> <salida.xml>
use invoice_rust::{clave_acceso::Ambiente, factura::*, firma::Firmador};

fn main() {
    let mut a = std::env::args().skip(1);
    let p12 = std::fs::read(a.next().expect("ruta .p12")).expect("leer .p12");
    let out = a.next().expect("archivo de salida");
    let pass = std::env::var("P12_PASSWORD").expect("variable P12_PASSWORD");
    let f = Factura {
        ambiente: Ambiente::Pruebas,
        emisor: Emisor {
            ruc: "1760013210001".into(),
            razon_social: "PRUEBAS".into(),
            nombre_comercial: None,
            dir_matriz: "QUITO".into(),
            dir_establecimiento: None,
            obligado_contabilidad: false,
            estab: "001".into(),
            pto_emi: "001".into(),
        },
        comprador: Comprador::consumidor_final(),
        fecha_emision: chrono_hoy(),
        secuencial: 1,
        codigo_numerico: "12345678".into(),
        lineas: vec![Linea { codigo_principal: "S/C".into(), descripcion: "ASISTENCIA".into(), cantidad: 1, precio_unitario: 1500, descuento: 0, iva: Iva::Quince }],
        forma_pago: "01".into(),
        info_adicional: vec![("Email".into(), "cliente@example.com".into())],
    };
    let (xml, clave) = f.to_xml().unwrap();
    let firmado = Firmador::desde_p12(&p12, &pass).expect("p12").firmar(&xml).expect("firmar");
    std::fs::write(&out, firmado).unwrap();
    println!("clave de acceso: {clave}\nfirmado -> {out}");
}

fn chrono_hoy() -> String {
    let o = std::process::Command::new("date").arg("+%d%m%Y").output().unwrap();
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}
