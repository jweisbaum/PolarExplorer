//! Hindsight Sync's local API: the one place the archive list comes from.
//!
//! Hindsight Sync is a separate desktop app that serves a read-only listing
//! of its archives on `127.0.0.1`. It refuses any request that carries an
//! `Origin` header, so this client sends no headers of its own, follows no
//! redirects and speaks plain HTTP. Credentials in a listing stay inside the
//! `ArchiveLocation`, whose `Debug` is redacted; nothing here prints them.
use crate::whirlwind::ArchiveLocation;
use crate::{EnvError, Result};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};

/// Hindsight Sync's default API port.
pub const DEFAULT_PORT: u16 = 8765;

/// Where an archive is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Storage {
    Cloud,
    Local,
}

/// An archive Hindsight Sync can read, with its location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedArchive {
    pub id: String,
    pub name: String,
    pub storage: Storage,
    pub uri: String,
    pub source_uri: Option<String>,
    pub mode: String,
    pub start: String,
    pub stop: Option<String>,
    pub layout_version: u32,
    pub chunk_degrees: u32,
    pub chunk_hours: u32,
    pub products: Vec<String>,
    pub location: ArchiveLocation,
}

impl ListedArchive {
    /// What the archive is, wherever it is kept: a local copy is its source's
    /// twin, so it answers with the source's address.
    pub fn identity(&self) -> &str {
        self.source_uri.as_deref().unwrap_or(&self.uri)
    }

    /// The host or folder a row shows.
    pub fn place(&self) -> String {
        self.location.place()
    }
}

/// An archive Hindsight Sync lists but cannot give a location for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub id: String,
    pub name: Option<String>,
    pub storage: Storage,
    pub reason: String,
}

/// Everything Hindsight Sync lists: cloud archives first, then local ones,
/// each in the server's order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub archives: Vec<ListedArchive>,
    pub unavailable: Vec<Unavailable>,
}

const NO_CREDENTIALS: &str = "credentials are not available";

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)?.as_str().map(str::to_owned)
}

fn number(v: &Value, key: &str) -> u32 {
    v.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

/// Asks one endpoint. `None` means "not detected": a transport error, a
/// status other than 200, a body that is not JSON, or no `api_version: 1`.
fn fetch(client: &Client, url: &str, timeout: Duration) -> Option<Value> {
    let response = client.get(url).timeout(timeout).send().ok()?;
    if response.status() != reqwest::StatusCode::OK {
        return None;
    }
    let body: Value = serde_json::from_slice(&response.bytes().ok()?).ok()?;
    (body.get("api_version").and_then(Value::as_u64) == Some(1)).then_some(body)
}

fn read(body: &Value, storage: Storage, into: &mut Listing) {
    for entry in body
        .get("archives")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = text(entry, "id").unwrap_or_default();
        let name = text(entry, "name").unwrap_or_default();
        let location_json = entry.get("location").cloned().unwrap_or(Value::Null);
        if storage == Storage::Cloud && location_json.get("credentials").is_none_or(Value::is_null)
        {
            let reason = location_json
                .get("credential_error")
                .and_then(Value::as_str)
                .unwrap_or(NO_CREDENTIALS)
                .to_owned();
            into.unavailable.push(Unavailable {
                id,
                name: Some(name),
                storage,
                reason,
            });
            continue;
        }
        let Ok(location) = serde_json::from_value::<ArchiveLocation>(location_json) else {
            into.unavailable.push(Unavailable {
                id,
                name: Some(name),
                storage,
                reason: "the archive's location is not understood".into(),
            });
            continue;
        };
        into.archives.push(ListedArchive {
            id,
            name,
            storage,
            uri: text(entry, "uri").unwrap_or_default(),
            source_uri: text(entry, "source_uri"),
            mode: text(entry, "mode").unwrap_or_default(),
            start: text(entry, "start_datetime").unwrap_or_default(),
            stop: text(entry, "stop_datetime"),
            layout_version: number(entry, "layout_version"),
            chunk_degrees: number(entry, "chunk_degrees"),
            chunk_hours: number(entry, "chunk_hours"),
            products: entry
                .get("products")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect(),
            location,
        });
    }
    for entry in body
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        into.unavailable.push(Unavailable {
            id: text(entry, "id").unwrap_or_default(),
            name: text(entry, "name"),
            storage,
            reason: text(entry, "error").unwrap_or_default(),
        });
    }
}

/// Asks Hindsight Sync on `127.0.0.1:{port}` for its reanalysis archives,
/// cloud then local, within about `timeout` in all.
///
/// `Ok(None)` is "not detected": the cloud endpoint was refused, timed out,
/// answered anything but 200, or did not say `api_version: 1`. The pair is
/// one detection: once the cloud endpoint answered the server is running, so
/// a local endpoint that then fails leaves the local list empty and still
/// returns `Ok(Some(..))`. Fields the API omits become empty values.
pub fn probe(port: u16, timeout: Duration) -> Result<Option<Listing>> {
    // The workspace builds reqwest without a TLS provider; a client cannot be
    // built until one is installed, even for plain HTTP. Already installed is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(timeout)
        .timeout(timeout)
        .build()
        .map_err(|e| EnvError::Open(format!("could not build the local client: {e}")))?;
    let started = Instant::now();
    let base = format!("http://127.0.0.1:{port}/v1/archives");
    let Some(cloud) = fetch(&client, &format!("{base}/cloud/reanalysis"), timeout) else {
        return Ok(None);
    };
    let mut listing = Listing::default();
    read(&cloud, Storage::Cloud, &mut listing);
    let left = timeout.saturating_sub(started.elapsed());
    if !left.is_zero()
        && let Some(local) = fetch(&client, &format!("{base}/local/reanalysis"), left)
    {
        read(&local, Storage::Local, &mut listing);
    }
    Ok(Some(listing))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Serves `(status, body)` to every request after `delay`, recording the
    /// raw request heads.
    fn stub(
        routes: Vec<(&'static str, u16, String)>,
        delay: Duration,
    ) -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let routes = routes.clone();
                let log = log.clone();
                std::thread::spawn(move || {
                    let mut buf = [0u8; 4096];
                    let n = s.read(&mut buf).unwrap_or(0);
                    let head = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
                    log.lock().unwrap().push(head);
                    std::thread::sleep(delay);
                    let (status, body) = routes
                        .iter()
                        .find(|(p, ..)| path.ends_with(p))
                        .map(|(_, c, b)| (*c, b.clone()))
                        .unwrap_or((404, String::new()));
                    let _ = write!(
                        s,
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        (port, seen)
    }

    fn cloud(id: &str, location: Value) -> Value {
        json!({"id": id, "name": id, "kind": "reanalysis", "storage": "cloud",
            "uri": format!("s3://{id}/p"), "mode": "global", "start_datetime": "2000-01-01T00:00:00Z",
            "stop_datetime": "2020-01-01T00:00:00Z", "chunk_degrees": 5, "chunk_hours": 24,
            "products": ["wind"], "parameters": [], "layout_version": 2, "location": location})
    }

    fn s3(endpoint: Value, region: &str, path_style: bool, creds: Value) -> Value {
        json!({"protocol": "s3", "provider": "x", "bucket": "b", "prefix": "p",
            "endpoint_url": endpoint, "region": region, "force_path_style": path_style,
            "credentials": creds})
    }

    fn fixture() -> Vec<(&'static str, u16, String)> {
        let keys = json!({"access_key_id": "AK", "secret_access_key": "SK", "session_token": null});
        let mut no_key = s3(Value::Null, "us-east-1", false, Value::Null);
        no_key["credential_error"] = json!("no AWS key");
        let cloud_body = json!({"api_version": 1,
            "archives": [
                cloud("aws", s3(Value::Null, "us-east-1", false, keys.clone())),
                cloud("r2", s3(json!(format!("{}://acct.r2.cloudflarestorage.com", "https")), "auto", true, keys)),
                cloud("nokey", no_key),
                cloud("anon", s3(Value::Null, "us-east-1", false, Value::Null))],
            "errors": [{"id": "broken", "storage": "cloud", "error": "bad metadata"}]});
        let mut local = cloud("copy", json!({"protocol": "file", "path": "/data/copy"}));
        local["storage"] = json!("local");
        local["uri"] = json!("/data/copy");
        local["source_uri"] = json!("s3://weather/source");
        local["backfilled_at"] = json!("2026-01-01T00:00:00Z");
        local["stop_datetime"] = Value::Null;
        let local_body = json!({"api_version": 1, "archives": [local], "errors": []});
        vec![
            ("/cloud/reanalysis", 200, cloud_body.to_string()),
            ("/local/reanalysis", 200, local_body.to_string()),
        ]
    }

    #[test]
    fn a_listing_keeps_cloud_then_local_and_sends_no_origin() {
        let (port, seen) = stub(fixture(), Duration::ZERO);
        let listing = probe(port, Duration::from_secs(5)).unwrap().unwrap();
        let ids: Vec<_> = listing.archives.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["aws", "r2", "copy"]);
        assert_eq!(listing.archives[2].storage, Storage::Local);
        assert_eq!(listing.archives[1].place(), "acct.r2.cloudflarestorage.com");
        assert_eq!(
            listing.archives[0].stop.as_deref(),
            Some("2020-01-01T00:00:00Z")
        );
        assert_eq!(listing.archives[2].stop, None);
        assert_eq!(listing.unavailable.len(), 3);
        assert_eq!(listing.unavailable[0].id, "nokey");
        assert_eq!(listing.unavailable[0].reason, "no AWS key");
        assert_eq!(listing.unavailable[1].id, "anon");
        assert_eq!(listing.unavailable[1].reason, NO_CREDENTIALS);
        assert_eq!(listing.unavailable[2].id, "broken");
        assert_eq!(listing.unavailable[2].name, None);
        assert_eq!(listing.unavailable[2].reason, "bad metadata");
        let heads = seen.lock().unwrap();
        assert_eq!(heads.len(), 2);
        for head in heads.iter() {
            assert!(!head.to_ascii_lowercase().contains("origin:"), "{head}");
        }
    }

    #[test]
    fn identity_prefers_the_local_copys_source() {
        let (port, _) = stub(fixture(), Duration::ZERO);
        let listing = probe(port, Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(listing.archives[2].identity(), "s3://weather/source");
        assert_eq!(listing.archives[0].identity(), "s3://aws/p");
    }

    #[test]
    fn not_detected_when_refused_wrong_version_or_not_json() {
        let closed = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        assert_eq!(probe(closed, Duration::from_secs(1)).unwrap(), None);
        for (code, body) in [
            (200, r#"{"api_version":2,"archives":[]}"#),
            (200, "<html>"),
            (403, r#"{"error":"local_applications_only"}"#),
        ] {
            let (port, _) = stub(
                vec![("/cloud/reanalysis", code, body.into())],
                Duration::ZERO,
            );
            assert_eq!(probe(port, Duration::from_secs(1)).unwrap(), None, "{body}");
        }
    }

    #[test]
    fn a_slow_server_is_not_detected_within_the_timeout() {
        let (port, _) = stub(fixture(), Duration::from_secs(3));
        let started = Instant::now();
        assert_eq!(probe(port, Duration::from_millis(500)).unwrap(), None);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }
}
