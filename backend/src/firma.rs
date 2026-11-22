//! Firma XAdES-BES enveloped (RSA-SHA1, C14N 20010315) según la ficha técnica del SRI,
//! anexo 14. Está pensada para XML generado por `factura::Factura::to_xml`, que ya sale
//! en forma canónica; no es un canonicalizador XML genérico.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rsa::{
    pkcs1v15::SigningKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
    traits::PublicKeyParts,
    RsaPrivateKey,
};
use sha1::{Digest, Sha1};
use x509_parser::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum FirmaError {
    #[error("no se pudo leer el .p12 (¿contraseña incorrecta?): {0}")]
    P12(String),
    #[error("el .p12 no contiene clave privada y certificado")]
    SinClave,
    #[error("clave privada no soportada (se requiere RSA): {0}")]
    Clave(String),
    #[error("certificado inválido: {0}")]
    Cert(String),
    #[error("el XML no termina en </factura>")]
    Xml,
}

pub struct Firmador {
    key: RsaPrivateKey,
    cert_der: Vec<u8>,
}

const NS: &str = r#" xmlns:ds="http://www.w3.org/2000/09/xmldsig#" xmlns:etsi="http://uri.etsi.org/01903/v1.3.2#""#;

fn sha1_b64(data: &[u8]) -> String {
    B64.encode(Sha1::digest(data))
}

fn ids() -> [u32; 8] {
    // IDs únicos por firma; no necesitan ser secretos.
    let mut s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64
        | 1;
    let mut out = [0u32; 8];
    for o in &mut out {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        *o = (s % 900_000) as u32 + 100_000;
    }
    out
}

impl Firmador {
    pub fn desde_p12(bytes: &[u8], password: &str) -> Result<Self, FirmaError> {
        let ks = p12_keystore::KeyStore::from_pkcs12(bytes, password, p12_keystore::Pkcs12ImportPolicy::Relaxed)
            .map_err(|e| FirmaError::P12(e.to_string()))?;
        let (_, chain) = ks.private_key_chain().ok_or(FirmaError::SinClave)?;
        let key = RsaPrivateKey::from_pkcs8_der(chain.key().as_der())
            .map_err(|e| FirmaError::Clave(e.to_string()))?;
        let cert_der = chain
            .certs()
            .first()
            .ok_or(FirmaError::SinClave)?
            .as_der()
            .to_vec();
        Ok(Self { key, cert_der })
    }

    /// Firma el XML de la factura y devuelve el documento completo (con declaración XML).
    pub fn firmar(&self, factura_xml: &str) -> Result<String, FirmaError> {
        let cuerpo = factura_xml.strip_suffix("</factura>").ok_or(FirmaError::Xml)?;
        let (_, cert) = X509Certificate::from_der(&self.cert_der)
            .map_err(|e| FirmaError::Cert(e.to_string()))?;

        let id = ids();
        let sig_id = format!("Signature{}", id[0]);
        let signed_info_id = format!("Signature-SignedInfo{}", id[1]);
        let sig_value_id = format!("SignatureValue{}", id[2]);
        let cert_id = format!("Certificate{}", id[3]);
        let props_id = format!("{sig_id}-SignedProperties{}", id[4]);
        let props_ref_id = format!("SignedPropertiesID{}", id[5]);
        let ref_id = format!("Reference-ID{}", id[6]);
        let obj_id = format!("{sig_id}-Object{}", id[7]);

        // Digest del comprobante (la firma enveloped no está presente aún).
        let digest_doc = sha1_b64(factura_xml.as_bytes());

        // KeyInfo
        let cert_b64 = B64.encode(&self.cert_der);
        let modulus = B64.encode(self.key.n().to_bytes_be());
        let exponent = B64.encode(self.key.e().to_bytes_be());
        let key_info_c14n = format!(
            r#"<ds:KeyInfo{NS} Id="{cert_id}"><ds:X509Data><ds:X509Certificate>{cert_b64}</ds:X509Certificate></ds:X509Data><ds:KeyValue><ds:RSAKeyValue><ds:Modulus>{modulus}</ds:Modulus><ds:Exponent>{exponent}</ds:Exponent></ds:RSAKeyValue></ds:KeyValue></ds:KeyInfo>"#
        );
        let digest_keyinfo = sha1_b64(key_info_c14n.as_bytes());

        // SignedProperties
        let ahora = chrono::Utc::now()
            .with_timezone(&chrono::FixedOffset::west_opt(5 * 3600).unwrap())
            .format("%Y-%m-%dT%H:%M:%S%:z");
        let issuer = issuer_rfc2253(&cert);
        let serial = cert.tbs_certificate.serial.to_string();
        let digest_cert = sha1_b64(&self.cert_der);
        let props_c14n = format!(
            r##"<etsi:SignedProperties{NS} Id="{props_id}"><etsi:SignedSignatureProperties><etsi:SigningTime>{ahora}</etsi:SigningTime><etsi:SigningCertificate><etsi:Cert><etsi:CertDigest><ds:DigestMethod Algorithm="http://www.w3.org/2000/09/xmldsig#sha1"></ds:DigestMethod><ds:DigestValue>{digest_cert}</ds:DigestValue></etsi:CertDigest><etsi:IssuerSerial><ds:X509IssuerName>{}</ds:X509IssuerName><ds:X509SerialNumber>{serial}</ds:X509SerialNumber></etsi:IssuerSerial></etsi:Cert></etsi:SigningCertificate></etsi:SignedSignatureProperties><etsi:SignedDataObjectProperties><etsi:DataObjectFormat ObjectReference="#{ref_id}"><etsi:Description>contenido comprobante</etsi:Description><etsi:MimeType>text/xml</etsi:MimeType></etsi:DataObjectFormat></etsi:SignedDataObjectProperties></etsi:SignedProperties>"##,
            crate::factura::esc(&issuer)
        );
        let digest_props = sha1_b64(props_c14n.as_bytes());

        // SignedInfo
        let signed_info_c14n = format!(
            r##"<ds:SignedInfo{NS} Id="{signed_info_id}"><ds:CanonicalizationMethod Algorithm="http://www.w3.org/TR/2001/REC-xml-c14n-20010315"></ds:CanonicalizationMethod><ds:SignatureMethod Algorithm="http://www.w3.org/2000/09/xmldsig#rsa-sha1"></ds:SignatureMethod><ds:Reference Id="{props_ref_id}" Type="http://uri.etsi.org/01903#SignedProperties" URI="#{props_id}"><ds:DigestMethod Algorithm="http://www.w3.org/2000/09/xmldsig#sha1"></ds:DigestMethod><ds:DigestValue>{digest_props}</ds:DigestValue></ds:Reference><ds:Reference URI="#{cert_id}"><ds:DigestMethod Algorithm="http://www.w3.org/2000/09/xmldsig#sha1"></ds:DigestMethod><ds:DigestValue>{digest_keyinfo}</ds:DigestValue></ds:Reference><ds:Reference Id="{ref_id}" URI="#comprobante"><ds:Transforms><ds:Transform Algorithm="http://www.w3.org/2000/09/xmldsig#enveloped-signature"></ds:Transform></ds:Transforms><ds:DigestMethod Algorithm="http://www.w3.org/2000/09/xmldsig#sha1"></ds:DigestMethod><ds:DigestValue>{digest_doc}</ds:DigestValue></ds:Reference></ds:SignedInfo>"##
        );
        let sig = SigningKey::<Sha1>::new(self.key.clone()).sign(signed_info_c14n.as_bytes());
        let sig_b64 = B64.encode(sig.to_bytes());

        // En el documento, los xmlns viven en <ds:Signature>; se quitan de los hijos.
        let sin_ns = |s: &str| s.replacen(NS, "", 1);
        Ok(format!(
            r##"<?xml version="1.0" encoding="UTF-8"?>
{cuerpo}<ds:Signature{NS} Id="{sig_id}">{}<ds:SignatureValue Id="{sig_value_id}">{sig_b64}</ds:SignatureValue>{}<ds:Object Id="{obj_id}"><etsi:QualifyingProperties Target="#{sig_id}">{}</etsi:QualifyingProperties></ds:Object></ds:Signature></factura>"##,
            sin_ns(&signed_info_c14n),
            sin_ns(&key_info_c14n),
            sin_ns(&props_c14n),
        ))
    }
}

/// Nombre del emisor al estilo RFC 2253 (el RDN más específico primero), como en el
/// ejemplo de la guía: `CN=...,OU=...,O=...,C=EC`.
fn issuer_rfc2253(cert: &X509Certificate) -> String {
    let mut partes = vec![];
    for rdn in cert.issuer().iter_rdn() {
        for atv in rdn.iter() {
            let nombre = x509_parser::objects::oid2abbrev(atv.attr_type(), x509_parser::objects::oid_registry())
                .unwrap_or("OID");
            let valor = atv.as_str().unwrap_or("");
            partes.push(format!("{nombre}={valor}"));
        }
    }
    partes.reverse();
    partes.join(",")
}
