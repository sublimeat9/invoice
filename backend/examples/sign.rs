//! P12_PASSWORD=... cargo run --example firmar -- <cert.p12> <salida.xml>
use invoice_rust::{access_key::Environment, invoice::*, signing::CertSigner};

fn main() {
    let mut a = std::env::args().skip(1);
    let p12 = std::fs::read(a.next().expect("ruta .p12")).expect("leer .p12");
    let out = a.next().expect("archivo de salida");
    let pass = std::env::var("P12_PASSWORD").expect("variable P12_PASSWORD");
    let f = Invoice {
        environment: Environment::Testing,
        issuer: Issuer {
            ruc: "1760013210001".into(),
            business_name: "PRUEBAS".into(),
            trade_name: None,
            head_office_address: "QUITO".into(),
            branch_address: None,
            accounting_required: false,
            establishment: "001".into(),
            emission_point: "001".into(),
        },
        buyer: Buyer::final_consumer(),
        issue_date: chrono_today(),
        sequence: 1,
        numeric_code: "12345678".into(),
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
    let (xml, access_key) = f.to_xml().unwrap();
    let signed = CertSigner::from_p12(&p12, &pass)
        .expect("p12")
        .sign(&xml)
        .expect("firmar");
    std::fs::write(&out, signed).unwrap();
    println!("clave de acceso: {access_key}\nfirmado -> {out}");
}

fn chrono_today() -> String {
    let o = std::process::Command::new("date")
        .arg("+%d%m%Y")
        .output()
        .unwrap();
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}
