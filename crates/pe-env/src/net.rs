//! The HTTP client every archive read goes through.
//!
//! reqwest's own `rustls` feature pulls in the aws-lc-rs crypto provider,
//! which is a C library; the five-target build allows none (D6). The client
//! is therefore built with `rustls-no-provider` and handed a `ClientConfig`
//! on the pure-Rust `ring` provider, verifying certificates against the
//! platform's own trust store.

use std::sync::Arc;
use std::time::Duration;

use rustls_platform_verifier::BuilderVerifierExt;

use crate::error::{EnvError, Result};

/// Sent with every request so archive operators can see who is reading.
pub const USER_AGENT: &str = concat!("PolarExplorer/", env!("CARGO_PKG_VERSION"));

/// The per-request timeout when the caller has no setting to pass
/// (spec.md 3.4 default).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// A TLS configuration on the `ring` provider with the platform verifier.
fn tls_config() -> Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| EnvError::Open(format!("TLS setup: {e}")))?
        .with_platform_verifier()
        .map_err(|e| EnvError::Open(format!("TLS certificate verifier: {e}")))?
        .with_no_client_auth();
    // A preconfigured config is used as given, so ALPN has to be set here for
    // HTTP/2 to be offered at all.
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

/// The most redirects one request follows.
const MAX_REDIRECTS: usize = 10;

/// The archive hosts (invariant 4; `tools/check-offline.sh` holds the same
/// list with the path prefix the archive lives under).
const HOSTS: [&str; 3] = [
    "whirlwind-hindsight.s3.us-east-1.amazonaws.com",
    "3d5456ad10ebc32c8a3c259aa84e6c64.r2.cloudflarestorage.com",
    "fly.storage.tigris.dev",
];

/// Whether a redirect from `from` to `to` may be followed: never to a
/// userinfo URL or from HTTPS down to HTTP; to the same host, or over HTTPS
/// to an archive host. Anything else would let a server send the app to a
/// host invariant 4 does not allow.
pub fn redirect_allowed(from: Option<&reqwest::Url>, to: &reqwest::Url) -> bool {
    let Some(host) = to.host_str() else {
        return false;
    };
    if !to.username().is_empty() || to.password().is_some() {
        return false;
    }
    let secure = to.scheme() == "https";
    if !secure && (to.scheme() != "http" || from.is_some_and(|f| f.scheme() == "https")) {
        return false;
    }
    let same_host = from
        .and_then(reqwest::Url::host_str)
        .is_some_and(|h| h.eq_ignore_ascii_case(host));
    same_host || (secure && HOSTS.iter().any(|h| h.eq_ignore_ascii_case(host)))
}

fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }
        if redirect_allowed(attempt.previous().last(), attempt.url()) {
            attempt.follow()
        } else {
            let to = attempt.url().host_str().unwrap_or("").to_owned();
            attempt.error(format!(
                "a redirect to {to:?}, which is not an allowed host"
            ))
        }
    })
}

/// A blocking client with the given per-request timeout, following
/// redirects only as [`redirect_allowed`] says.
///
/// # Errors
/// [`EnvError::Open`] if TLS or the client cannot be set up.
pub fn client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .tls_backend_preconfigured(tls_config()?)
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .redirect(redirect_policy())
        .build()
        .map_err(|e| EnvError::Open(format!("no HTTP client: {e}")))
}

/// Authenticated S3 requests never follow redirects: signatures and tokens
/// belong to the exact bucket endpoint they were made for.
pub(crate) fn s3_client(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls_config()?)
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| EnvError::Open(format!("no S3 client: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building the client exercises the ring provider and the platform
    /// verifier without touching the network.
    #[test]
    fn a_client_builds_on_the_ring_provider() {
        assert!(client(DEFAULT_TIMEOUT).is_ok());
    }

    fn url(text: &str) -> reqwest::Url {
        reqwest::Url::parse(text).expect(text)
    }

    #[test]
    fn redirects_stay_on_the_host_or_the_archive_hosts() {
        let from =
            url("https://whirlwind-hindsight.s3.us-east-1.amazonaws.com/hindsight-routing/x");
        assert!(redirect_allowed(
            Some(&from),
            &url("https://whirlwind-hindsight.s3.us-east-1.amazonaws.com/hindsight-routing/other")
        ));
        assert!(redirect_allowed(
            Some(&from),
            &url("https://fly.storage.tigris.dev/whirlwind-hindsight/hindsight-routing/x")
        ));
        for to in [
            "https://evil.invalid/x",
            "https://whirlwind-hindsight.s3.us-east-1.amazonaws.com.evil.invalid/x",
            &format!(
                "{}user@whirlwind-hindsight.s3.us-east-1.amazonaws.com/hindsight-routing/x",
                "https://"
            ),
            "http://whirlwind-hindsight.s3.us-east-1.amazonaws.com/hindsight-routing/x",
            "file:///etc/passwd",
        ] {
            assert!(!redirect_allowed(Some(&from), &url(to)), "{to}");
        }
        let local = url("http://127.0.0.1:8080/a");
        assert!(redirect_allowed(Some(&local), &url("http://127.0.0.1:9/b")));
        assert!(!redirect_allowed(
            Some(&local),
            &url("http://evil.invalid/b")
        ));
    }

    /// End to end through a local server: a redirect on the same host is
    /// followed; one to another host is refused without being requested.
    #[test]
    fn the_client_follows_only_allowed_redirects() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
        let port = listener.local_addr().expect("an address").port();
        let server = std::thread::spawn(move || {
            for _ in 0..3 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
                let reply = match path.as_str() {
                    "/same" => format!(
                        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                    "/away" => "HTTP/1.1 302 Found\r\nLocation: http://evil.invalid/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
                    _ => "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_owned(),
                };
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        let client = client(Duration::from_secs(5)).expect("a client");
        let body = client
            .get(format!("http://127.0.0.1:{port}/same"))
            .send()
            .and_then(reqwest::blocking::Response::text)
            .expect("followed");
        assert_eq!(body, "ok");
        let err = client
            .get(format!("http://127.0.0.1:{port}/away"))
            .send()
            .expect_err("refused");
        assert!(err.is_redirect(), "{err}");
        drop(client);
        // Unblock the server's last accept.
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        server.join().expect("server");
    }
}
