//! HTTP client builder for controller API calls.
//!
//! All controller endpoints expect the SDK to authenticate over mTLS
//! using the identity's client cert + private key. The Ziti controller
//! also presents a self-signed TLS server cert, so the identity's CA
//! bundle must be added to the HTTP client's root store — without this,
//! every reqwest call to the controller fails the TLS handshake before
//! it can even send the request.
//!
//! [`controller_client`] returns a [`reqwest::Client`] that:
//!   - trusts the CA(s) from `identity.config().ca`,
//!   - presents the identity cert + key from `identity.config().cert`
//!     and `identity.config().key` as the mTLS client identity,
//!   - applies the supplied request timeout.

use crate::error::{ZitiError, ZitiResult};
use crate::identity::IdentityManager;
use reqwest::{Certificate, Client, Identity};
use std::io::BufReader;
use std::time::Duration;
use tokio::fs;

/// Build a reqwest client configured for controller API calls.
pub(crate) async fn controller_client(
    identity: &IdentityManager,
    timeout: Duration,
) -> ZitiResult<Client> {
    let cfg = identity.config();
    let mut builder = Client::builder().timeout(timeout);

    let ca_pem = fs::read(&cfg.ca).await.map_err(|e| {
        ZitiError::ConfigError(format!("Failed to read CA file {}: {}", cfg.ca, e))
    })?;
    let mut ca_reader = BufReader::new(ca_pem.as_slice());
    let cas: Vec<_> = rustls_pemfile::certs(&mut ca_reader)
        .collect::<Result<_, _>>()
        .map_err(|e| ZitiError::ConfigError(format!("Failed to parse CA PEM bundle: {}", e)))?;
    if cas.is_empty() {
        return Err(ZitiError::ConfigError(format!(
            "CA bundle {} contained no certificates",
            cfg.ca
        )));
    }
    for ca in cas {
        let cert = Certificate::from_der(ca.as_ref())
            .map_err(|e| ZitiError::ConfigError(format!("Failed to add CA to client: {}", e)))?;
        builder = builder.add_root_certificate(cert);
    }

    let cert_pem = fs::read(&cfg.cert).await.map_err(|e| {
        ZitiError::ConfigError(format!("Failed to read cert file {}: {}", cfg.cert, e))
    })?;
    let key_pem = fs::read(&cfg.key).await.map_err(|e| {
        ZitiError::ConfigError(format!("Failed to read key file {}: {}", cfg.key, e))
    })?;
    let mut bundle = Vec::with_capacity(cert_pem.len() + key_pem.len() + 1);
    bundle.extend_from_slice(&cert_pem);
    if !cert_pem.ends_with(b"\n") {
        bundle.push(b'\n');
    }
    bundle.extend_from_slice(&key_pem);
    let id = Identity::from_pem(&bundle)
        .map_err(|e| ZitiError::ConfigError(format!("Failed to build mTLS identity: {}", e)))?;
    builder = builder.identity(id);

    builder
        .build()
        .map_err(|e| ZitiError::ConfigError(format!("Failed to build HTTP client: {}", e)))
}
