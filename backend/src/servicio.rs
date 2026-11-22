//! Capa de servicio: reserva el secuencial, genera, firma, guarda, envía y registra el
//! resultado. Un comprobante firmado se guarda como `PENDIENTE` *antes* de enviarlo, de
//! modo que si el proceso muere o el SRI no responde se puede reintentar con el mismo XML
//! (misma clave de acceso) con `reintentar_pendientes`.

use crate::{
    clave_acceso::{codigo_numerico_aleatorio, Ambiente},
    factura::{Comprador, Emisor, Factura, Linea},
    firma::{FirmaError, Firmador},
    sri::{Autorizacion, Mensaje, SriClient, SriError},
    store::{Registro, Store, StoreError},
};

#[derive(Debug, thiserror::Error)]
pub enum ServicioError {
    #[error("datos inválidos: {0}")]
    Datos(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Firma(#[from] FirmaError),
    #[error("SRI: {0}")]
    Sri(#[from] SriError),
}

pub struct Servicio {
    pub store: Store,
    pub firmador: Firmador,
    pub sri: SriClient,
    pub ambiente: Ambiente,
    pub emisor: Emisor,
}

pub struct SolicitudFactura {
    pub comprador: Comprador,
    pub lineas: Vec<Linea>,
    pub forma_pago: String,
    pub info_adicional: Vec<(String, String)>,
}

fn hoy_ecuador() -> String {
    chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::west_opt(5 * 3600).unwrap())
        .format("%d%m%Y")
        .to_string()
}

fn resumen(ms: &[Mensaje]) -> Option<String> {
    (!ms.is_empty()).then(|| {
        ms.iter()
            .map(|m| {
                format!(
                    "[{}] {}: {} {}",
                    m.tipo,
                    m.identificador,
                    m.mensaje,
                    m.informacion_adicional.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

impl Servicio {
    /// Emite una factura de punta a punta y devuelve el registro final guardado.
    pub async fn emitir(&self, s: SolicitudFactura) -> Result<Registro, ServicioError> {
        self.emisor.validar().map_err(ServicioError::Datos)?;
        let mut factura = Factura {
            ambiente: self.ambiente,
            emisor: self.emisor.clone(),
            comprador: s.comprador,
            fecha_emision: hoy_ecuador(),
            secuencial: 1, // provisional: solo para validar antes de gastar un secuencial
            codigo_numerico: codigo_numerico_aleatorio(),
            lineas: s.lineas,
            forma_pago: s.forma_pago,
            info_adicional: s.info_adicional,
        };
        factura.to_xml().map_err(ServicioError::Datos)?;

        factura.secuencial =
            self.store.reservar_secuencial(self.ambiente, &self.emisor.estab, &self.emisor.pto_emi)?;
        let (xml, clave) = factura.to_xml().map_err(ServicioError::Datos)?;
        let firmado = self.firmador.firmar(&xml)?;
        self.store.guardar_pendiente(&clave, factura.secuencial, &firmado)?;

        self.enviar(&clave, &firmado).await
    }

    /// Reintenta los comprobantes que quedaron `PENDIENTE`. Devuelve el resultado de cada
    /// uno; un fallo de red en uno no detiene a los demás (queda `PENDIENTE`).
    pub async fn reintentar_pendientes(&self) -> Result<Vec<(String, Result<Registro, ServicioError>)>, ServicioError> {
        let mut out = vec![];
        for p in self.store.pendientes()? {
            let r = self.enviar(&p.clave_acceso, &p.xml_firmado).await;
            out.push((p.clave_acceso, r));
        }
        Ok(out)
    }

    /// Envía y registra el resultado. Si el SRI no responde (error de red/servicio), el
    /// comprobante queda `PENDIENTE` —con el motivo en `mensajes`— y se devuelve igualmente
    /// el registro, para que quien llama conozca la clave de acceso y pueda reintentar.
    async fn enviar(&self, clave: &str, firmado: &str) -> Result<Registro, ServicioError> {
        let rec = match self.sri.recepcion(firmado.as_bytes()).await {
            Ok(r) => r,
            Err(e) => return self.dejar_pendiente(clave, e),
        };
        if !rec.recibida() && !rec.ya_registrada() {
            // Rechazo definitivo en recepción: no tiene sentido reintentar el mismo XML.
            self.store
                .actualizar_estado(clave, &rec.estado, None, None, resumen(&rec.mensajes).as_deref())?;
            return Ok(self.registro(clave)?);
        }
        let au: Autorizacion = match self
            .sri
            .esperar_autorizacion(clave, std::time::Duration::from_secs(3), 10)
            .await
        {
            Ok(a) => a,
            Err(e) => return self.dejar_pendiente(clave, e),
        };
        self.store.actualizar_estado(
            clave,
            &au.estado,
            au.numero_autorizacion.as_deref(),
            au.fecha_autorizacion.as_deref(),
            resumen(&au.mensajes).as_deref(),
        )?;
        Ok(self.registro(clave)?)
    }

    fn dejar_pendiente(&self, clave: &str, e: SriError) -> Result<Registro, ServicioError> {
        self.store
            .actualizar_estado(clave, "PENDIENTE", None, None, Some(&format!("último intento: {e}")))?;
        Ok(self.registro(clave)?)
    }

    fn registro(&self, clave: &str) -> Result<Registro, StoreError> {
        Ok(self.store.obtener(clave)?.expect("comprobante recién guardado"))
    }
}
