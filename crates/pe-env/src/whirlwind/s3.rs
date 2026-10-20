//! Read-only, bucket-scoped transport: anonymous or signed reads of any
//! S3-compatible location. Secrets never enter URLs or errors.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue};
use sha2::{Digest, Sha256};

use super::{ArchiveLocation, Credentials};
use crate::{EnvError, Result};

/// How many times a transient failure is tried again before it is reported.
const RETRIES: u32 = 3;

/// Counters for what went over the network.
#[derive(Debug, Default)]
pub(super) struct NetStats {
    /// Requests sent, including ones answered "missing".
    pub requests: AtomicU64,
    /// Body bytes received.
    pub bytes: AtomicU64,
    /// Keys the archive answered 403 or 404 for.
    pub missing: AtomicU64,
    /// Requests sent again after a transient failure.
    pub retries: AtomicU64,
}

impl NetStats {
    /// `(requests, bytes, missing)` at this moment.
    pub fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.requests.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
            self.missing.load(Ordering::Relaxed),
        )
    }
}

const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

impl Credentials {
    fn headers(
        &self,
        url: &reqwest::Url,
        region: &str,
        range: Option<&str>,
        timestamp: &str,
    ) -> Result<HeaderMap> {
        let date = &timestamp[..8];
        let scope = format!("{date}/{region}/s3/aws4_request");
        let host = url
            .host_str()
            .ok_or_else(|| EnvError::Open("invalid S3 endpoint".into()))?;
        // The Host header names a port that is not the scheme's default.
        let host = url
            .port()
            .map_or_else(|| host.to_owned(), |p| format!("{host}:{p}"));
        let mut canonical = format!("host:{host}\n");
        let mut signed = "host".to_owned();
        if let Some(range) = range {
            canonical.push_str(&format!("range:{range}\n"));
            signed.push_str(";range");
        }
        canonical.push_str(&format!(
            "x-amz-content-sha256:{EMPTY_HASH}\nx-amz-date:{timestamp}\n"
        ));
        signed.push_str(";x-amz-content-sha256;x-amz-date");
        if let Some(token) = &self.session_token {
            canonical.push_str(&format!("x-amz-security-token:{token}\n"));
            signed.push_str(";x-amz-security-token");
        }
        let request = format!("GET\n{}\n\n{canonical}\n{signed}\n{EMPTY_HASH}", url.path());
        let hash = format!("{:x}", Sha256::digest(request.as_bytes()));
        let to_sign = format!("AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{hash}");
        let mut key = mac(format!("AWS4{}", self.secret_access_key).as_bytes(), date)?;
        for part in [region, "s3", "aws4_request"] {
            key = mac(&key, part)?;
        }
        let signature: String = mac(&key, &to_sign)?
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect();
        let auth = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
            self.access_key_id
        );
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("x-amz-content-sha256", EMPTY_HASH),
            ("x-amz-date", timestamp),
            ("authorization", &auth),
        ] {
            let mut value = HeaderValue::from_str(value)
                .map_err(|_| EnvError::Open("invalid Whirlwind credentials".into()))?;
            value.set_sensitive(name == "authorization");
            headers.insert(name, value);
        }
        if let Some(token) = &self.session_token {
            let mut value = HeaderValue::from_str(token)
                .map_err(|_| EnvError::Open("invalid Whirlwind session token".into()))?;
            value.set_sensitive(true);
            headers.insert("x-amz-security-token", value);
        }
        if let Some(range) = range {
            headers.insert(
                "range",
                HeaderValue::from_str(range)
                    .map_err(|_| EnvError::Layout("invalid byte range".into()))?,
            );
        }
        Ok(headers)
    }
}

fn mac(key: &[u8], text: &str) -> Result<Vec<u8>> {
    let mut h = Hmac::<Sha256>::new_from_slice(key)
        .map_err(|_| EnvError::Open("S3 signing failed".into()))?;
    h.update(text.as_bytes());
    Ok(h.finalize().into_bytes().to_vec())
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Range {
    Bytes(u64, u64),
    Suffix(u64),
}

/// A single shared async connection pool and request limit for every job.
#[derive(Debug)]
pub(super) struct S3 {
    /// The object root, with no trailing `/`.
    base: String,
    region: String,
    client: reqwest::Client,
    credentials: Option<Credentials>,
    pub stats: Arc<NetStats>,
    permits: tokio::sync::Semaphore,
}

#[derive(Debug)]
pub(super) struct Object {
    pub bytes: Bytes,
    pub etag: Option<String>,
}

impl S3 {
    pub fn new(location: &ArchiveLocation, timeout: Duration, concurrency: usize) -> Result<Self> {
        let base = location.base_url()?;
        let ArchiveLocation::S3 {
            region,
            credentials,
            ..
        } = location
        else {
            return Err(EnvError::Open(
                "a folder archive is not read over S3".into(),
            ));
        };
        Ok(Self {
            base,
            region: region.clone(),
            client: crate::net::s3_client(timeout)?,
            credentials: credentials.clone(),
            stats: Arc::new(NetStats::default()),
            permits: tokio::sync::Semaphore::new(concurrency.max(1)),
        })
    }

    pub async fn get_versioned(
        &self,
        key: &str,
        range: Option<Range>,
        cancel: &AtomicBool,
    ) -> Result<Option<Object>> {
        // Only our fixed metadata and numerical chunk keys can be requested.
        if key.contains("..")
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_./".contains(&c))
        {
            return Err(EnvError::Layout("invalid Whirlwind object key".into()));
        }
        let range_text = range.map(|r| match r {
            Range::Bytes(start, len) => format!("bytes={start}-{}", start + len - 1),
            Range::Suffix(len) => format!("bytes=-{len}"),
        });
        let limit = match range {
            Some(Range::Bytes(_, n) | Range::Suffix(n)) => n,
            None => 16 << 20,
        };
        if limit == 0 || limit > 16 << 20 {
            return Err(EnvError::Layout(
                "Whirlwind object exceeds the read limit".into(),
            ));
        }
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| EnvError::Cancelled)?;
        for attempt in 0..=RETRIES {
            if cancel.load(Ordering::SeqCst) {
                return Err(EnvError::Cancelled);
            }
            let url = reqwest::Url::parse(&format!("{}/{key}", self.base))
                .map_err(|_| EnvError::Open("invalid S3 endpoint".into()))?;
            let headers = if let Some(credentials) = &self.credentials {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let timestamp = crate::time::to_iso(now).replace(['-', ':'], "");
                credentials.headers(&url, &self.region, range_text.as_deref(), &timestamp)?
            } else {
                let mut headers = HeaderMap::new();
                if let Some(range) = &range_text {
                    headers.insert(
                        "range",
                        HeaderValue::from_str(range)
                            .map_err(|_| EnvError::Layout("invalid byte range".into()))?,
                    );
                }
                headers
            };
            let url = url.to_string();
            self.stats.requests.fetch_add(1, Ordering::Relaxed);
            let request = async {
                let mut response = self.client.get(&url).headers(headers).send().await?;
                let status = response.status();
                if status == reqwest::StatusCode::NOT_FOUND {
                    return Ok((status, None, None, None));
                }
                if status == reqwest::StatusCode::BAD_REQUEST {
                    // AWS answers an expired or unknown key with a 400 whose
                    // error code is the only sign it is the credentials.
                    let mut body = Vec::new();
                    while let Some(chunk) = response.chunk().await? {
                        body.extend_from_slice(&chunk);
                        if body.len() > ERROR_BODY_LIMIT {
                            break;
                        }
                    }
                    return Ok((status, Some(Bytes::from(body)), None, None));
                }
                if !status.is_success() {
                    return Ok((status, None, None, None));
                }
                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let content_range = response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let mut bytes = Vec::new();
                // Refuse a server that ignores Range before buffering its shard.
                if (range.is_some() && status != reqwest::StatusCode::PARTIAL_CONTENT)
                    || response.content_length().is_some_and(|n| n > limit)
                {
                    return Ok((status, None, content_range, etag));
                }
                while let Some(chunk) = response.chunk().await? {
                    self.stats
                        .bytes
                        .fetch_add(chunk.len() as u64, Ordering::Relaxed);
                    if bytes.len() as u64 + chunk.len() as u64 > limit {
                        return Ok((status, None, content_range, etag));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok::<_, reqwest::Error>((status, Some(Bytes::from(bytes)), content_range, etag))
            };
            let result = tokio::select! {
                result = request => result,
                () = cancelled(cancel) => return Err(EnvError::Cancelled),
            };
            let retry = match result {
                Ok((reqwest::StatusCode::NOT_FOUND, _, _, _)) => {
                    self.stats.missing.fetch_add(1, Ordering::Relaxed);
                    return Ok(None);
                }
                Ok((status, bytes, content_range, etag)) if status.is_success() => {
                    let bytes = bytes.ok_or_else(|| {
                        EnvError::Layout("S3 ignored the byte range or exceeded its size".into())
                    })?;
                    if let Some(range) = range {
                        validate_range(range, content_range.as_deref(), bytes.len() as u64)?;
                    }
                    return Ok(Some(Object { bytes, etag }));
                }
                Ok((status, body, _, _)) => {
                    if refused_for_credentials(status.as_u16(), body.as_deref()) {
                        return Err(EnvError::Auth {
                            status: status.as_u16(),
                            key: key.to_owned(),
                        });
                    }
                    if !status.is_server_error() && status.as_u16() != 429 && status.as_u16() != 408
                    {
                        return Err(EnvError::Open(format!(
                            "the archive answered {status} for {key}"
                        )));
                    }
                    true
                }
                Err(error) => {
                    error.is_timeout()
                        || error.is_connect()
                        || error.is_body()
                        || error.is_request()
                }
            };
            if !retry || attempt == RETRIES {
                break;
            }
            self.stats.retries.fetch_add(1, Ordering::Relaxed);
            tokio::select! {
                () = tokio::time::sleep(Duration::from_millis(500 << attempt)) => {},
                () = cancelled(cancel) => return Err(EnvError::Cancelled),
            }
        }
        Err(EnvError::Open(format!(
            "the archive request failed for {key} after retries"
        )))
    }
}

/// How much of a 400's body is read to find its error code.
const ERROR_BODY_LIMIT: usize = 8 << 10;

/// The S3 error codes of a 400 that mean the credentials, not the request.
const CREDENTIAL_CODES: [&str; 3] = [
    "<Code>ExpiredToken</Code>",
    "<Code>InvalidAccessKeyId</Code>",
    "<Code>SignatureDoesNotMatch</Code>",
];

/// Whether an answer refuses the read for want of valid credentials, which
/// a Scan in Settings can refresh.
fn refused_for_credentials(status: u16, body: Option<&[u8]>) -> bool {
    match status {
        401 | 403 => true,
        400 => body.is_some_and(|body| {
            let text = String::from_utf8_lossy(body);
            CREDENTIAL_CODES.iter().any(|code| text.contains(code))
        }),
        _ => false,
    }
}

async fn cancelled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn validate_range(range: Range, header: Option<&str>, length: u64) -> Result<()> {
    let parsed = header
        .and_then(|h| h.strip_prefix("bytes "))
        .and_then(|h| h.split_once('/'))
        .and_then(|(r, n)| r.split_once('-').map(|(a, b)| (a, b, n)))
        .and_then(|(a, b, n)| {
            Some((
                a.parse::<u64>().ok()?,
                b.parse::<u64>().ok()?,
                n.parse::<u64>().ok()?,
            ))
        });
    let valid = parsed.is_some_and(|(a, b, n)| {
        b < n
            && b.checked_sub(a).and_then(|v| v.checked_add(1)) == Some(length)
            && match range {
                Range::Bytes(start, len) => a == start && length == len,
                Range::Suffix(len) => b + 1 == n && length == len,
            }
    });
    if valid {
        Ok(())
    } else {
        Err(EnvError::Layout(
            "S3 returned an incomplete or incorrect byte range".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incorrect_or_truncated_ranges_fail() {
        assert!(validate_range(Range::Bytes(10, 20), Some("bytes 10-29/100"), 20).is_ok());
        assert!(validate_range(Range::Suffix(20), Some("bytes 80-99/100"), 20).is_ok());
        for header in [
            None,
            Some("bytes 10-28/100"),
            Some("bytes 11-30/100"),
            Some("bytes 10-29/*"),
        ] {
            assert!(validate_range(Range::Bytes(10, 20), header, 20).is_err());
        }
        assert!(validate_range(Range::Suffix(20), Some("bytes 79-98/100"), 20).is_err());
    }

    /// AWS's documented error responses (S3 API reference, "Error
    /// responses"): the codes that mean the key, not the request.
    #[test]
    fn only_a_credentials_answer_is_a_refusal_for_credentials() {
        let aws = |code: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Error><Code>{code}</Code><Message>m</Message></Error>"
            )
        };
        assert!(refused_for_credentials(401, None));
        assert!(refused_for_credentials(403, None));
        assert!(refused_for_credentials(
            403,
            Some(aws("AccessDenied").as_bytes())
        ));
        for code in [
            "ExpiredToken",
            "InvalidAccessKeyId",
            "SignatureDoesNotMatch",
        ] {
            assert!(
                refused_for_credentials(400, Some(aws(code).as_bytes())),
                "{code}"
            );
        }
        for (status, body) in [
            (400, Some(aws("InvalidArgument"))),
            (400, Some(aws("InvalidRange"))),
            (400, None),
            (404, Some(aws("NoSuchKey"))),
            (409, Some(aws("ExpiredToken"))),
            (500, None),
        ] {
            assert!(
                !refused_for_credentials(status, body.as_deref().map(str::as_bytes)),
                "{status} {body:?}"
            );
        }
    }

    #[test]
    fn credential_debug_and_errors_never_contain_secrets() {
        let mut c = Credentials::new("TEST", "secret-never-print");
        c.session_token = Some("token-never-print".into());
        let location = ArchiveLocation::S3 {
            bucket: "whirlwind-hindsight".into(),
            prefix: "hindsight-routing".into(),
            endpoint_url: None,
            region: "us-east-1".into(),
            force_path_style: false,
            credentials: None,
        };
        let debug = format!("{c:?}");
        assert!(!debug.contains("secret-never-print") && !debug.contains("token-never-print"));
        let headers = c
            .headers(
                &reqwest::Url::parse(&format!("{}/data/c/0/0/1/2", location.base_url().unwrap()))
                    .unwrap(),
                "us-east-1",
                Some("bytes=0-63"),
                "20261007T010203Z",
            )
            .unwrap();
        assert!(headers["authorization"].is_sensitive());
        assert!(headers["x-amz-security-token"].is_sensitive());
        let auth = headers["authorization"].to_str().unwrap();
        assert!(auth.contains("20261007/us-east-1/s3/aws4_request"));
        assert!(auth.contains(
            "SignedHeaders=host;range;x-amz-content-sha256;x-amz-date;x-amz-security-token"
        ));
        assert!(!auth.contains("secret-never-print"));
    }
}
