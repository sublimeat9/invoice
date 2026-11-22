//! Capa de servicio: reserva el secuencial, genera, firma, guarda, envía y registra el
//! resultado. Un comprobante firmado se guarda como `PENDIENTE` *antes* de enviarlo, de
//! modo que si el proceso muere o el SRI no responde se puede reintentar con el mismo XML
//! (misma clave de acceso) con `retry_pending`.

use crate::{
    access_key::{random_numeric_code, Environment},
    crypto::{self, EncryptionKey},
    invoice::{Buyer, Invoice, Issuer, Line},
    signing::{CertSigner, SigningError},
    sri::{Authorization, Message, SriClient, SriError},
    store::{Meta, Record, Store, StoreError},
};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("datos inválidos: {0}")]
    InvalidData(String),
    /// Falta configuración (emisor, certificado) o el certificado no es utilizable.
    #[error("configuración incompleta: {0}")]
    NotConfigured(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Signing(#[from] SigningError),
    #[error("SRI: {0}")]
    Sri(#[from] SriError),
}

pub struct Service {
    pub store: Store,
    encryption_key: EncryptionKey,
    /// Solo para pruebas: apunta el cliente SOAP a un SRI simulado.
    sri_base: Option<String>,
    /// Solo para pruebas: apunta la consulta de RUC a un servidor simulado.
    catastro_base: Option<String>,
}

/// Configuración vigente, leída de la base en cada operación (así los cambios hechos desde
/// la interfaz se aplican sin reiniciar).
pub struct Config {
    pub issuer: Issuer,
    pub environment: Environment,
    pub signer: CertSigner,
}

pub struct InvoiceRequest {
    pub buyer: Buyer,
    pub lines: Vec<Line>,
    pub payment_method: String,
    pub additional_info: Vec<(String, String)>,
}

fn today_in_ecuador() -> String {
    chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::west_opt(5 * 3600).unwrap())
        .format("%d%m%Y")
        .to_string()
}

fn summary(ms: &[Message]) -> Option<String> {
    (!ms.is_empty()).then(|| {
        ms.iter()
            .map(|m| {
                format!(
                    "[{}] {}: {} {}",
                    m.kind,
                    m.identifier,
                    m.message,
                    m.additional_information.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// El dígito 24 de la clave de acceso es el ambiente (1 pruebas, 2 producción). Se usa para
/// reenviar siempre al ambiente en que se firmó el comprobante, aunque la configuración
/// haya cambiado después.
fn environment_from_key(access_key: &str) -> Environment {
    if access_key.as_bytes().get(23) == Some(&b'2') {
        Environment::Production
    } else {
        Environment::Testing
    }
}

impl Service {
    /// `secret` es `ENCRYPTION_KEY`: cifra el certificado guardado en la base.
    pub fn new(store: Store, secret: &str) -> Self {
        Self {
            store,
            encryption_key: crypto::derive_key(secret),
            sri_base: None,
            catastro_base: None,
        }
    }

    pub fn with_catastro_base(mut self, base: String) -> Self {
        self.catastro_base = Some(base);
        self
    }

    pub fn catastro(&self) -> crate::catastro::CatastroClient {
        crate::catastro::CatastroClient::new(self.catastro_base.clone())
    }

    pub fn con_sri_base(mut self, base: String) -> Self {
        self.sri_base = Some(base);
        self
    }

    fn sri(&self, a: Environment) -> SriClient {
        match &self.sri_base {
            Some(b) => SriClient::with_base(b.clone()),
            None => SriClient::new(a),
        }
    }

    pub fn encryption_key(&self) -> &EncryptionKey {
        &self.encryption_key
    }

    /// Carga y valida la configuración necesaria para emitir.
    pub fn config(&self, owner: i64) -> Result<Config, ServiceError> {
        let (issuer, environment) = self
            .store
            .load_issuer(owner)?
            .ok_or_else(|| ServiceError::NotConfigured("faltan los datos del emisor".into()))?;
        issuer.validate().map_err(ServiceError::NotConfigured)?;
        let (p12, pass) = self
            .store
            .load_certificate(owner, &self.encryption_key)
            .map_err(ServiceError::NotConfigured)?
            .ok_or_else(|| {
                ServiceError::NotConfigured("falta cargar el certificado .p12".into())
            })?;
        let signer = CertSigner::from_p12(&p12, &pass)?;
        if !signer.info()?.valid {
            return Err(ServiceError::NotConfigured(
                "el certificado está vencido".into(),
            ));
        }
        Ok(Config {
            issuer,
            environment,
            signer,
        })
    }

    /// Emite una factura de punta a punta y devuelve el registro final guardado.
    pub async fn issue(&self, owner: i64, s: InvoiceRequest) -> Result<Record, ServiceError> {
        let cfg = self.config(owner)?;
        let mut invoice = Invoice {
            environment: cfg.environment,
            issuer: cfg.issuer.clone(),
            buyer: s.buyer,
            issue_date: today_in_ecuador(),
            sequence: 1, // provisional: solo para validar antes de gastar un secuencial
            numeric_code: random_numeric_code(),
            lines: s.lines,
            payment_method: s.payment_method,
            additional_info: s.additional_info,
        };
        invoice.to_xml().map_err(ServiceError::InvalidData)?;

        invoice.sequence = self.store.reserve_sequence(
            owner,
            cfg.environment,
            &cfg.issuer.establishment,
            &cfg.issuer.emission_point,
        )?;
        let (xml, access_key) = invoice.to_xml().map_err(ServiceError::InvalidData)?;
        let signed = cfg.signer.sign(&xml)?;
        let meta = Meta {
            issue_date: invoice.iso_date(),
            document_type: "01".into(),
            id_number: invoice.buyer.id_number.clone(),
            business_name: invoice.buyer.business_name.clone(),
            total_amount: invoice.grand_total(),
        };
        self.store
            .save_pending(owner, &access_key, invoice.sequence, &signed, &meta)?;

        self.send(&access_key, &signed).await
    }

    /// Reintenta los comprobantes que quedaron `PENDIENTE`. Devuelve el resultado de cada
    /// uno; un fallo de red en uno no detiene a los demás (queda `PENDIENTE`).
    /// `owner`: solo los de ese usuario; `None`: los de todos (reintento en segundo plano).
    pub async fn retry_pending(
        &self,
        owner: Option<i64>,
    ) -> Result<Vec<(String, Result<Record, ServiceError>)>, ServiceError> {
        let mut out = vec![];
        for p in self.store.pending(owner)? {
            let r = self.send(&p.access_key, &p.signed_xml).await;
            out.push((p.access_key, r));
        }
        Ok(out)
    }

    /// Envía y registra el resultado. Si el SRI no responde (error de red/servicio), el
    /// comprobante queda `PENDIENTE` —con el motivo en `messages`— y se devuelve igualmente
    /// el registro, para que quien llama conozca la clave de acceso y pueda reintentar.
    async fn send(&self, access_key: &str, signed: &str) -> Result<Record, ServiceError> {
        let sri = self.sri(environment_from_key(access_key));
        let reception = match sri.reception(signed.as_bytes()).await {
            Ok(r) => r,
            Err(e) => return self.leave_pending(access_key, e),
        };
        if !reception.received() && !reception.already_registered() {
            // Rechazo definitivo en recepción: no tiene sentido reintentar el mismo XML.
            self.store.update_status(
                access_key,
                &reception.status,
                None,
                None,
                summary(&reception.messages).as_deref(),
            )?;
            return Ok(self.record(access_key)?);
        }
        let authorization: Authorization = match sri
            .wait_for_authorization(access_key, std::time::Duration::from_secs(3), 10)
            .await
        {
            Ok(a) => a,
            Err(e) => return self.leave_pending(access_key, e),
        };
        self.store.update_status(
            access_key,
            &authorization.status,
            authorization.authorization_number.as_deref(),
            authorization.authorization_date.as_deref(),
            summary(&authorization.messages).as_deref(),
        )?;
        Ok(self.record(access_key)?)
    }

    fn leave_pending(&self, access_key: &str, e: SriError) -> Result<Record, ServiceError> {
        self.store.update_status(
            access_key,
            "PENDIENTE",
            None,
            None,
            Some(&format!("último intento: {e}")),
        )?;
        Ok(self.record(access_key)?)
    }

    fn record(&self, access_key: &str) -> Result<Record, StoreError> {
        Ok(self
            .store
            .get_record_any(access_key)?
            .expect("comprobante recién guardado"))
    }
}
