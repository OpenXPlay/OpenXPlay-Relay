// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Start a TLS Lockdown session and keep it alive while the carkit stream is owned.
use super::{
    iap2_usb_mux_host::{Iap2UsbMuxHost, LOCKDOWN_PORT},
    lockdown_pair_record::{decode_pem, LockdownPairRecord},
    lockdown_pairing_client::{reject_error, LockdownPairingError, PairingResult},
    lockdown_plist_channel::{dictionary, LockdownPlistChannel},
    lockdown_tls_engine_factory::TlsConnector,
};
use crate::{invalid, DuplexStream, Receive, Result};
use plist::Value;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
pub struct LockdownCarKitClient {
    host: Arc<Iap2UsbMuxHost>,
    tls: Arc<dyn TlsConnector>,
}
impl LockdownCarKitClient {
    pub fn new(host: Arc<Iap2UsbMuxHost>, tls: Arc<dyn TlsConnector>) -> Self {
        Self { host, tls }
    }
    pub fn open(
        &self,
        record: &LockdownPairRecord,
        label: &str,
    ) -> PairingResult<Arc<CarkitServiceStream>> {
        self.open_service(record, label, "com.apple.carkit.service")
    }
    pub fn open_service(
        &self,
        record: &LockdownPairRecord,
        label: &str,
        service_name: &str,
    ) -> PairingResult<Arc<CarkitServiceStream>> {
        if label.trim().is_empty() || service_name.trim().is_empty() {
            return Err(invalid("label and service name must not be blank").into());
        }
        let timeout = Duration::from_secs(5);
        let connection = self.host.connect(LOCKDOWN_PORT, timeout)?;
        let plaintext = LockdownPlistChannel::new(connection);
        let response = plaintext.request(
            &dictionary([
                ("Label", Value::String(label.into())),
                ("Request", Value::String("StartSession".into())),
                ("HostID", Value::String(record.host_id.clone())),
                ("SystemBUID", Value::String(record.system_buid.clone())),
            ]),
            timeout,
        )?;
        reject_error(&response)?;
        match response.get("EnableSessionSSL").and_then(Value::as_boolean) {
            Some(true) => (),
            Some(false) => return Err(invalid("StartSession did not enable TLS").into()),
            None => {
                return Err(
                    invalid("StartSession response omitted boolean EnableSessionSSL").into(),
                )
            }
        }
        let session_tls = self.wrap(plaintext.detach()?, record)?;
        let lockdown = LockdownPlistChannel::new(session_tls);
        let response = lockdown.request(
            &dictionary([
                ("Request", Value::String("StartService".into())),
                ("Service", Value::String(service_name.into())),
            ]),
            timeout,
        )?;
        reject_error(&response)?;
        let port = response
            .get("Port")
            .and_then(Value::as_unsigned_integer)
            .filter(|p| (1..=65535).contains(p))
            .ok_or(LockdownPairingError::InvalidResponse(
                "StartService returned an invalid Port",
            ))? as u16;
        let connection = self.host.connect(port, timeout)?;
        let service: Arc<dyn DuplexStream> = if response
            .get("EnableServiceSSL")
            .and_then(Value::as_boolean)
            .unwrap_or(false)
        {
            self.wrap(connection, record)?
        } else {
            connection
        };
        Ok(Arc::new(CarkitServiceStream {
            service,
            lockdown,
            write: Mutex::new(()),
            closed: AtomicBool::new(false),
        }))
    }
    fn wrap(
        &self,
        stream: Arc<dyn DuplexStream>,
        record: &LockdownPairRecord,
    ) -> Result<Arc<dyn DuplexStream>> {
        let identity = (|| {
            let certificate =
                CertificateDer::from(decode_pem("CERTIFICATE", &record.root_certificate_pem)?);
            let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(decode_pem(
                "PRIVATE KEY",
                &record.root_private_key_pem,
            )?));
            self.tls.connect(stream.clone(), vec![certificate], key)
        })();
        if identity.is_err() {
            let _ = stream.close();
        }
        identity
    }
}
/// Carkit plus its creating Lockdown session. Sends are serialized in 256-byte chunks.
pub struct CarkitServiceStream {
    service: Arc<dyn DuplexStream>,
    lockdown: LockdownPlistChannel,
    write: Mutex<()>,
    closed: AtomicBool,
}
impl DuplexStream for CarkitServiceStream {
    fn send(&self, data: &[u8]) -> Result<()> {
        let _guard = self.write.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(invalid("Carkit service stream is closed"));
        }
        for chunk in data.chunks(256) {
            self.service.send(chunk)?;
        }
        Ok(())
    }
    fn recv(&self, max_bytes: usize, timeout: Duration) -> Result<Receive> {
        self.service.recv(max_bytes, timeout)
    }
    fn close(&self) -> Result<()> {
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let result = self.service.close();
        let lockdown = self.lockdown.close();
        result.and(lockdown)
    }
}
impl Drop for CarkitServiceStream {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
