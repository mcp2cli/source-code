//! Shared TLS trust-store construction for every outbound HTTPS client
//! mcp2cli makes: the MCP Streamable HTTP transport
//! ([`crate::mcp::client::StreamableHttpMcpClient`]), telemetry shipping
//! ([`crate::telemetry`]), and OAuth discovery/token exchange
//! ([`crate::auth::oauth`]).
//!
//! Every client trusts the bundled Mozilla root set from `webpki-roots`
//! by default — the binary needs no system trust store to talk to a real
//! MCP server. When the `SSL_CERT_FILE` environment variable is set (the
//! same convention curl, Python `requests`, Go's `net/http`, and Ruby's
//! OpenSSL bindings honor), its PEM certificates are parsed and *added
//! to* that same trust store, so a corporate TLS-inspection proxy
//! (mitmproxy, ZScaler, Fortinet, …) configured once via the environment
//! is trusted everywhere mcp2cli makes an HTTPS connection. The bundled
//! roots are never removed — unsetting `SSL_CERT_FILE` restores exactly
//! the previous behavior.

use rustls::{ClientConfig, RootCertStore};

/// Build the root certificate store: bundled webpki roots, plus any PEM
/// certificates found at `SSL_CERT_FILE`, if set. A missing or unreadable
/// `SSL_CERT_FILE` degrades to the bundled roots with a warning rather
/// than failing outbound connections outright.
pub fn root_cert_store() -> RootCertStore {
    let mut store = RootCertStore::empty();
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let Ok(path) = std::env::var("SSL_CERT_FILE") else {
        return store;
    };

    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) => {
            tracing::warn!(
                path = %path,
                %error,
                "SSL_CERT_FILE is set but could not be opened; using bundled CA roots only"
            );
            return store;
        }
    };

    let mut reader = std::io::BufReader::new(file);
    let (mut added, mut failed) = (0usize, 0usize);
    for cert in rustls_pemfile::certs(&mut reader) {
        match cert.and_then(|cert| {
            store
                .add(cert)
                .map_err(|error| std::io::Error::other(error.to_string()))
        }) {
            Ok(()) => added += 1,
            Err(_) => failed += 1,
        }
    }

    if added > 0 {
        tracing::debug!(
            path = %path,
            added,
            "loaded extra CA certificate(s) from SSL_CERT_FILE"
        );
    }
    if failed > 0 {
        tracing::warn!(
            path = %path,
            failed,
            "SSL_CERT_FILE contained certificate(s) that could not be parsed or trusted"
        );
    }

    store
}

/// Build a rustls `ClientConfig` from [`root_cert_store`] — the shape
/// both the hyper (MCP transport) and ureq (telemetry, OAuth) HTTPS
/// clients need.
pub fn client_config() -> ClientConfig {
    ClientConfig::builder()
        .with_root_certificates(root_cert_store())
        .with_no_client_auth()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate the process-wide `SSL_CERT_FILE`
    /// environment variable, matching the pattern already used for
    /// telemetry's env-var tests (`MCP2CLI_TELEMETRY`, `DO_NOT_TRACK`).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// A minimal, syntactically valid self-signed certificate. Not used to
    /// terminate any real TLS connection here — only to exercise PEM
    /// parsing and root-store insertion.
    const TEST_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIDJTCCAg2gAwIBAgIUKoP2olDnPz+quw74YYdwLfdHmgYwDQYJKoZIhvcNAQEL\n\
BQAwIjEgMB4GA1UEAwwXbWNwMmNsaS10ZXN0LWZpeHR1cmUtY2EwHhcNMjYwNzIz\n\
MTUxMTE0WhcNMzYwNzIwMTUxMTE0WjAiMSAwHgYDVQQDDBdtY3AyY2xpLXRlc3Qt\n\
Zml4dHVyZS1jYTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBALRtn8Zj\n\
fsmQUM4y7k1108LUSjgCz7ynwVFkfHsv72IrY7sx8q5mvMmH9E5EHTsCsfqG3EZ2\n\
wjp/tjQQLKgn38o9vFsul3S/RGEzAMsGaFLiBpR3jWqa3ML/i4NLxwu0i4sspRXZ\n\
jMpC+8FGLibzMASqL4yEvtvOG4XsxOhKaZO0wh63+MbN+3Ms26Txr9sg5e6OBq5h\n\
1qLkvil8OUAaOyb507mkwZPe7inlbf8dZTp6KRa83mit3rju5/krhX0FFcynKksS\n\
48snc+zbfKS2ITb0R8GpQr16WxudCOvn6rHsecoA2kRLMaysva96d7ucaGMSSy5l\n\
dyOBZZu4Xuv0vcsCAwEAAaNTMFEwHQYDVR0OBBYEFF/UcaHqa63cZPt7PremB+Dc\n\
EonuMB8GA1UdIwQYMBaAFF/UcaHqa63cZPt7PremB+DcEonuMA8GA1UdEwEB/wQF\n\
MAMBAf8wDQYJKoZIhvcNAQELBQADggEBAEKN/vLE9NpG8CMj/ipJfKVtIlcugJ5n\n\
bCMgiUl3/C4qXrFNSH6itXk5AB1vOP5TeBuXF5nE4y/JuTba36f6pHbTckdxFsJ5\n\
8CSZuil5gjCBkexxt+DKFYvAlpbPzyCyAsp9oOyA5NgmZxyP1mUUzOeNwcJEllNP\n\
9ekxrrz+sBTYgwl5OPqJaM9Afl2c9Ti6xzii5vkWiZMZLhg4SYUNooKKFuDBxqp2\n\
SXRHntkPClUZKsCShJ2XddA8j96/9HBm6OB3Y7MJtZ3DlSVMztshLAZL1yFwSZk2\n\
NJpTsURtKDUZUCIZL6pRNwD50Il3lhH+MB+EmaFnqfM6JuYHJsuti/Y=\n\
-----END CERTIFICATE-----\n";

    fn clear_ssl_cert_file() {
        // SAFETY: test-only; ENV_LOCK serializes every test in this module
        // that touches SSL_CERT_FILE.
        unsafe {
            std::env::remove_var("SSL_CERT_FILE");
        }
    }

    #[test]
    fn without_ssl_cert_file_store_matches_bundled_roots() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        clear_ssl_cert_file();

        let store = root_cert_store();
        assert_eq!(store.roots.len(), webpki_roots::TLS_SERVER_ROOTS.len());
    }

    #[test]
    fn ssl_cert_file_adds_certificates_on_top_of_bundled_roots() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let cert_path = dir.path().join("custom-ca.pem");
        std::fs::write(&cert_path, TEST_CERT_PEM).expect("cert fixture should write");

        // SAFETY: test-only; guarded by ENV_LOCK above.
        unsafe {
            std::env::set_var("SSL_CERT_FILE", &cert_path);
        }
        let store = root_cert_store();
        unsafe {
            std::env::remove_var("SSL_CERT_FILE");
        }

        assert_eq!(store.roots.len(), webpki_roots::TLS_SERVER_ROOTS.len() + 1);
    }

    #[test]
    fn missing_ssl_cert_file_degrades_to_bundled_roots_only() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        // SAFETY: test-only; guarded by ENV_LOCK above.
        unsafe {
            std::env::set_var("SSL_CERT_FILE", "/nonexistent/path/does-not-exist.pem");
        }
        let store = root_cert_store();
        unsafe {
            std::env::remove_var("SSL_CERT_FILE");
        }

        assert_eq!(store.roots.len(), webpki_roots::TLS_SERVER_ROOTS.len());
    }

    #[test]
    fn unparsable_ssl_cert_file_degrades_to_bundled_roots_only() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let cert_path = dir.path().join("garbage.pem");
        std::fs::write(&cert_path, b"not a certificate").expect("fixture should write");

        // SAFETY: test-only; guarded by ENV_LOCK above.
        unsafe {
            std::env::set_var("SSL_CERT_FILE", &cert_path);
        }
        let store = root_cert_store();
        unsafe {
            std::env::remove_var("SSL_CERT_FILE");
        }

        assert_eq!(store.roots.len(), webpki_roots::TLS_SERVER_ROOTS.len());
    }

    #[test]
    fn client_config_builds_without_panicking() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        clear_ssl_cert_file();
        let _ = client_config();
    }
}
