//! Clave de acceso de 49 dígitos (ficha técnica SRI v2.34, tabla 1).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoComprobante {
    Factura,
    LiquidacionCompra,
    NotaCredito,
    NotaDebito,
    GuiaRemision,
    Retencion,
}

impl TipoComprobante {
    pub fn codigo(self) -> &'static str {
        match self {
            Self::Factura => "01",
            Self::LiquidacionCompra => "03",
            Self::NotaCredito => "04",
            Self::NotaDebito => "05",
            Self::GuiaRemision => "06",
            Self::Retencion => "07",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ambiente {
    Pruebas,
    Produccion,
}

impl Ambiente {
    pub fn codigo(self) -> char {
        match self {
            Self::Pruebas => '1',
            Self::Produccion => '2',
        }
    }
}

/// Dígito verificador módulo 11 con pesos 2..7 de derecha a izquierda.
pub fn modulo11(digitos: &str) -> u8 {
    let suma: u32 = digitos
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| (b - b'0') as u32 * (2 + (i as u32 % 6)))
        .sum();
    match 11 - (suma % 11) {
        11 => 0,
        10 => 1,
        d => d as u8,
    }
}

/// Código numérico de 8 dígitos aleatorio (el algoritmo es potestad del emisor, sec. 5.2).
pub fn codigo_numerico_aleatorio() -> String {
    use rand::Rng;
    format!("{:08}", rand::thread_rng().gen_range(0..100_000_000u32))
}

pub struct ClaveAccesoParams<'a> {
    /// Formato `ddmmaaaa`.
    pub fecha_emision: &'a str,
    pub tipo: TipoComprobante,
    pub ruc: &'a str,
    pub ambiente: Ambiente,
    /// Establecimiento + punto de emisión, p. ej. `001001`.
    pub serie: &'a str,
    pub secuencial: u32,
    /// 8 dígitos, a elección del emisor.
    pub codigo_numerico: &'a str,
}

/// Tipo de emisión: 1 = normal (única opción vigente).
pub fn generar(p: &ClaveAccesoParams) -> Result<String, String> {
    let digits = |s: &str, n: usize, name: &str| {
        if s.len() == n && s.bytes().all(|b| b.is_ascii_digit()) {
            Ok(())
        } else {
            Err(format!("{name} debe tener {n} dígitos: {s:?}"))
        }
    };
    digits(p.fecha_emision, 8, "fecha_emision")?;
    digits(p.ruc, 13, "ruc")?;
    digits(p.serie, 6, "serie")?;
    digits(p.codigo_numerico, 8, "codigo_numerico")?;
    if p.secuencial == 0 || p.secuencial > 999_999_999 {
        return Err("secuencial fuera de rango 1..=999999999".into());
    }
    let base = format!(
        "{}{}{}{}{}{:09}{}1",
        p.fecha_emision,
        p.tipo.codigo(),
        p.ruc,
        p.ambiente.codigo(),
        p.serie,
        p.secuencial,
        p.codigo_numerico,
    );
    debug_assert_eq!(base.len(), 48);
    Ok(format!("{base}{}", modulo11(&base)))
}

pub fn validar(clave: &str) -> bool {
    clave.len() == 49
        && clave.bytes().all(|b| b.is_ascii_digit())
        && modulo11(&clave[..48]) == clave.as_bytes()[48] - b'0'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codigo_numerico_tiene_8_digitos() {
        for _ in 0..100 {
            let c = codigo_numerico_aleatorio();
            assert_eq!(c.len(), 8);
            assert!(c.bytes().all(|b| b.is_ascii_digit()));
        }
    }

    #[test]
    fn ejemplo_de_la_guia() {
        assert_eq!(modulo11("41261533"), 6);
    }

    #[test]
    fn claves_de_ejemplo_de_la_guia_son_validas() {
        assert!(validar("0503201201176001321000110010030009900641234567814"));
        assert!(validar("1302201201176001321000120010030000050431234567814"));
        assert!(!validar("1302201201176001321000120010030000050431234567815"));
    }

    #[test]
    fn generar_reproduce_ejemplo() {
        let c = generar(&ClaveAccesoParams {
            fecha_emision: "05032012",
            tipo: TipoComprobante::Factura,
            ruc: "1760013210001",
            ambiente: Ambiente::Pruebas,
            serie: "001003",
            secuencial: 990064,
            codigo_numerico: "12345678",
        })
        .unwrap();
        assert_eq!(c, "0503201201176001321000110010030009900641234567814");
    }
}
