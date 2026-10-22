//! Where a Whirlwind archive is: any location Hindsight Sync lists, read
//! straight from its JSON (`location`), so a new bucket or provider needs no
//! code. Credentials arrive with the location, live only in memory and the
//! transport, and never reach IPC, URLs, projects or logs.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{EnvError, Result};

/// S3 credentials for one archive. `Debug` never shows them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub(super) access_key_id: String,
    pub(super) secret_access_key: String,
    #[serde(default)]
    pub(super) session_token: Option<String>,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}

impl Credentials {
    /// A key pair without a session token.
    pub fn new(access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
        }
    }
}

/// An archive's place: Hindsight Sync's `location` object, tagged by
/// `protocol`. Fields Hindsight Sync adds that the reader has no use for
/// (`provider`, `s3_uri`, `credential_error`) are ignored when read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "lowercase")]
pub enum ArchiveLocation {
    /// An S3-compatible bucket: Amazon S3 when `endpoint_url` is absent,
    /// otherwise R2, Tigris or any other S3 service at that endpoint.
    S3 {
        bucket: String,
        prefix: String,
        #[serde(default)]
        endpoint_url: Option<String>,
        /// The signing region: AWS's own, or `auto` for R2 and Tigris.
        region: String,
        #[serde(default)]
        force_path_style: bool,
        /// None reads anonymously.
        #[serde(default)]
        credentials: Option<Credentials>,
    },
    /// A folder on this computer.
    File { path: PathBuf },
}

/// `a/b` joined without doubled or trailing slashes; an empty `b` is `a`.
fn join(root: &str, prefix: &str) -> String {
    let root = root.trim_end_matches('/');
    let prefix = prefix.trim_matches('/');
    if prefix.is_empty() {
        root.to_owned()
    } else {
        format!("{root}/{prefix}")
    }
}

fn endpoint(url: &str) -> Result<reqwest::Url> {
    reqwest::Url::parse(url.trim_end_matches('/'))
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
        .ok_or_else(|| EnvError::Open("the archive's S3 endpoint is not an http(s) address".into()))
}

fn provider_name(host: &str) -> &'static str {
    if host.contains(".r2.cloudflarestorage.") {
        "R2"
    } else if host == "tigris.dev" || host.ends_with(".tigris.dev") {
        "Tigris"
    } else {
        "S3"
    }
}

impl ArchiveLocation {
    /// The object root: no endpoint → `https://{bucket}.s3.{region}.amazonaws.com/{prefix}`;
    /// endpoint + path style → `{endpoint}/{bucket}/{prefix}`; endpoint without path style →
    /// `https://{bucket}.{endpoint host}/{prefix}`. A trailing `/` on prefix or endpoint is dropped.
    ///
    /// # Errors
    /// A folder location, or an endpoint that is not an http(s) address.
    pub fn base_url(&self) -> Result<String> {
        match self {
            Self::S3 {
                bucket,
                prefix,
                endpoint_url: None,
                region,
                ..
            } => Ok(join(
                &format!("https://{bucket}.s3.{region}.amazonaws.com"),
                prefix,
            )),
            Self::S3 {
                bucket,
                prefix,
                endpoint_url: Some(url),
                force_path_style,
                ..
            } => {
                let parsed = endpoint(url)?;
                if *force_path_style {
                    Ok(join(&join(url, bucket), prefix))
                } else {
                    let port = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
                    let host = parsed.host_str().unwrap_or_default();
                    Ok(join(
                        &format!("{}://{bucket}.{host}{port}", parsed.scheme()),
                        prefix,
                    ))
                }
            }
            Self::File { .. } => Err(EnvError::Open(
                "a folder archive is not read over S3".into(),
            )),
        }
    }

    /// A short display label, without bucket, endpoint or folder paths.
    pub fn place(&self) -> String {
        match self {
            Self::S3 {
                endpoint_url: Some(url),
                ..
            } => {
                let host = endpoint(url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned))
                    .unwrap_or_default();
                provider_name(&host).to_owned()
            }
            Self::S3 { .. } => "S3".to_owned(),
            Self::File { path } => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        }
    }
}

/// An archive and the name its values are recorded under: the provenance
/// written on each sample, and the namespace of every cached chunk, so two
/// archives never share either.
///
/// Serialised, it carries the location's credentials in clear: it is
/// written only to the application's owner-only location file, never sent
/// over IPC or saved in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveRef {
    pub identity: String,
    pub location: ArchiveLocation,
}

#[cfg(test)]
mod tests {
    #[test]
    fn cloud_hostnames_have_short_provider_labels() {
        for (host, label) in [
            ("acct.r2.cloudflarestorage.com", "R2"),
            ("fly.storage.tigris.dev", "Tigris"),
            ("tigris.dev", "Tigris"),
            ("bucket.s3.us-east-1.amazonaws.com", "S3"),
            ("store.example.invalid", "S3"),
        ] {
            assert_eq!(super::provider_name(host), label);
        }
    }
}
