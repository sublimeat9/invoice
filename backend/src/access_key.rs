//! Clave de acceso de 49 dígitos (ficha técnica SRI v2.34, tabla 1).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentType {
    Invoice,
    PurchaseSettlement,
    CreditNote,
    DebitNote,
    DispatchGuide,
    Withholding,
}

impl DocumentType {
    pub fn code(self) -> &'static str {
        match self {
            Self::Invoice => "01",
            Self::PurchaseSettlement => "03",
            Self::CreditNote => "04",
            Self::DebitNote => "05",
            Self::DispatchGuide => "06",
            Self::Withholding => "07",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Testing,
    Production,
}

impl Environment {
    pub fn code(self) -> char {
        match self {
            Self::Testing => '1',
            Self::Production => '2',
        }
    }
}

/// Dígito verificador módulo 11 con pesos 2..7 de derecha a izquierda.
pub fn modulo11(digits: &str) -> u8 {
    let sum: u32 = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| (b - b'0') as u32 * (2 + (i as u32 % 6)))
        .sum();
    match 11 - (sum % 11) {
        11 => 0,
        10 => 1,
        d => d as u8,
    }
}

/// Código numérico de 8 dígitos aleatorio (el algoritmo es potestad del emisor, sec. 5.2).
pub fn random_numeric_code() -> String {
    use rand::Rng;
    format!("{:08}", rand::thread_rng().gen_range(0..100_000_000u32))
}

pub struct AccessKeyParams<'a> {
    /// Formato `ddmmaaaa`.
    pub issue_date: &'a str,
    pub kind: DocumentType,
    pub ruc: &'a str,
    pub environment: Environment,
    /// Establecimiento + punto de emisión, p. ej. `001001`.
    pub series: &'a str,
    pub sequence: u32,
    /// 8 dígitos, a elección del emisor.
    pub numeric_code: &'a str,
}

/// Tipo de emisión: 1 = normal (única opción vigente).
pub fn generate(p: &AccessKeyParams) -> Result<String, String> {
    let digits = |s: &str, n: usize, name: &str| {
        if s.len() == n && s.bytes().all(|b| b.is_ascii_digit()) {
            Ok(())
        } else {
            Err(format!("{name} debe tener {n} dígitos: {s:?}"))
        }
    };
    digits(p.issue_date, 8, "fecha_emision")?;
    digits(p.ruc, 13, "ruc")?;
    digits(p.series, 6, "serie")?;
    digits(p.numeric_code, 8, "codigo_numerico")?;
    if p.sequence == 0 || p.sequence > 999_999_999 {
        return Err("secuencial fuera de rango 1..=999999999".into());
    }
    let base = format!(
        "{}{}{}{}{}{:09}{}1",
        p.issue_date,
        p.kind.code(),
        p.ruc,
        p.environment.code(),
        p.series,
        p.sequence,
        p.numeric_code,
    );
    debug_assert_eq!(base.len(), 48);
    Ok(format!("{base}{}", modulo11(&base)))
}

pub fn validate(access_key: &str) -> bool {
    access_key.len() == 49
        && access_key.bytes().all(|b| b.is_ascii_digit())
        && modulo11(&access_key[..48]) == access_key.as_bytes()[48] - b'0'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_code_has_8_digits() {
        for _ in 0..100 {
            let c = random_numeric_code();
            assert_eq!(c.len(), 8);
            assert!(c.bytes().all(|b| b.is_ascii_digit()));
        }
    }

    #[test]
    fn guide_example() {
        assert_eq!(modulo11("41261533"), 6);
    }

    #[test]
    fn guide_example_keys_are_valid() {
        assert!(validate(
            "0503201201176001321000110010030009900641234567814"
        ));
        assert!(validate(
            "1302201201176001321000120010030000050431234567814"
        ));
        assert!(!validate(
            "1302201201176001321000120010030000050431234567815"
        ));
    }

    #[test]
    fn generate_reproduces_example() {
        let c = generate(&AccessKeyParams {
            issue_date: "05032012",
            kind: DocumentType::Invoice,
            ruc: "1760013210001",
            environment: Environment::Testing,
            series: "001003",
            sequence: 990064,
            numeric_code: "12345678",
        })
        .unwrap();
        assert_eq!(c, "0503201201176001321000110010030009900641234567814");
    }
}
