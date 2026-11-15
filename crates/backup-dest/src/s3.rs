//! An S3-compatible bucket (AWS, MinIO, Backblaze, Wasabi, Ceph ...), by plain HTTP
//! requests signed with AWS Signature Version 4.
//!
//! Objects up to one part (16 MiB) are written with one `PUT`; bigger ones with a
//! multipart upload, so a part of many gigabytes is never held in memory (one part is),
//! and an upload that fails is aborted so no half object is left billing.

use std::io::Read;

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

use crate::{DestError, Destination, check};

type HmacSha256 = Hmac<Sha256>;

/// Bytes in one part of a multipart upload.
pub const PART_BYTES: usize = 16 * 1024 * 1024;
const MAX_PARTS: usize = 10_000;

/// The settings and credentials of a bucket.
#[derive(Debug, Clone)]
pub struct S3Config {
    /// `https://s3.example.com` (no trailing slash).
    pub endpoint: String,
    /// The bucket.
    pub bucket: String,
    /// The region used in the signature.
    pub region: String,
    /// A folder inside the bucket (no slashes at the ends; may be empty).
    pub prefix: String,
    /// `endpoint/bucket/key` (true) or `bucket.endpoint/key` (false).
    pub path_style: bool,
    /// The access key.
    pub access_key: String,
    /// The secret key.
    pub secret_key: String,
}

/// A bucket.
pub struct S3 {
    config: S3Config,
    agent: ureq::Agent,
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).unwrap_or_else(|_| unreachable_key());
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// HMAC accepts any key length; this cannot happen.
fn unreachable_key() -> HmacSha256 {
    HmacSha256::new_from_slice(&[0]).unwrap_or_else(|_| std::process::abort())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Percent-encodes as SigV4 wants: everything but `A-Z a-z 0-9 - _ . ~`, and `/` kept when
/// `keep_slash`.
fn encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'_' | b'.' | b'~')
            || (keep_slash && b == b'/')
        {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// What goes into the signature of one request.
pub struct Signed<'a> {
    /// `GET`, `PUT` ...
    pub method: &'a str,
    /// The host header.
    pub host: &'a str,
    /// The path as sent (already encoded).
    pub path: &'a str,
    /// The query as sent, `a=b&c=d` (already encoded), sorted by the caller.
    pub query: &'a str,
    /// SHA-256 of the body, in hex.
    pub payload_hash: &'a str,
    /// `YYYYMMDDTHHMMSSZ`.
    pub amz_date: &'a str,
    /// Extra headers that are signed (lower-case name, trimmed value), sorted by name.
    pub extra: &'a [(&'a str, &'a str)],
}

/// The `Authorization` header value.
#[must_use]
pub fn authorization(s: &Signed<'_>, region: &str, access_key: &str, secret_key: &str) -> String {
    let date = &s.amz_date[..8];
    let mut headers: Vec<(&str, &str)> = vec![
        ("host", s.host),
        ("x-amz-content-sha256", s.payload_hash),
        ("x-amz-date", s.amz_date),
    ];
    headers.extend_from_slice(s.extra);
    headers.sort_by_key(|(k, _)| *k);
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join(";");
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        s.method, s.path, s.query, canonical_headers, signed_headers, s.payload_hash
    );
    let scope = format!("{date}/{region}/s3/aws4_request");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
        s.amz_date,
        sha_hex(canonical.as_bytes())
    );
    let k_date = hmac(format!("AWS4{secret_key}").as_bytes(), date.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, b"s3");
    let k_signing = hmac(&k_service, b"aws4_request");
    let signature = hex(&hmac(&k_signing, to_sign.as_bytes()));
    format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    )
}

fn amz_now() -> String {
    // Days since the epoch to a civil date (proleptic Gregorian), no time crate needed.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

impl S3 {
    /// A bucket.
    ///
    /// # Errors
    /// [`DestError::Config`] for settings that cannot be used.
    pub fn new(config: S3Config) -> Result<Self, DestError> {
        let local = config.endpoint.starts_with("http://127.0.0.1")
            || config.endpoint.starts_with("http://localhost");
        if !(config.endpoint.starts_with("https://") || local) || config.endpoint.ends_with('/') {
            return Err(DestError::Config(
                "the endpoint must be an https address".into(),
            ));
        }
        if config.bucket.is_empty() || config.access_key.is_empty() || config.secret_key.is_empty()
        {
            return Err(DestError::Config("bucket and keys are needed".into()));
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(std::time::Duration::from_secs(20)))
            .timeout_recv_response(Some(std::time::Duration::from_secs(300)))
            .max_redirects(0)
            .build()
            .into();
        Ok(Self { config, agent })
    }

    /// Makes the bucket (for a server that needs it made; an existing one is fine).
    ///
    /// # Errors
    /// [`DestError`].
    pub fn create_bucket(&self) -> Result<(), DestError> {
        let (status, response) = self.call("PUT", "", &[], Some(&[]))?;
        match status {
            200 | 409 => Ok(()),
            _ => Err(refusal(status, response)),
        }
    }

    /// `(url, host, path)` for an object key (or the bucket itself when `key` is empty).
    fn target(&self, key: &str) -> (String, String, String) {
        // An empty key is the bucket itself (listing, making it); the prefix belongs to objects.
        let full = if self.config.prefix.is_empty() || key.is_empty() {
            key.to_owned()
        } else {
            format!("{}/{key}", self.config.prefix)
        };
        let (scheme, host_part) = self
            .config
            .endpoint
            .split_once("://")
            .unwrap_or(("https", &self.config.endpoint));
        if self.config.path_style {
            let path = if full.is_empty() {
                format!("/{}", self.config.bucket)
            } else {
                format!("/{}/{}", self.config.bucket, encode(&full, true))
            };
            (
                format!("{scheme}://{host_part}{path}"),
                host_part.to_owned(),
                path,
            )
        } else {
            let host = format!("{}.{host_part}", self.config.bucket);
            let path = if full.is_empty() {
                "/".to_owned()
            } else {
                format!("/{}", encode(&full, true))
            };
            (format!("{scheme}://{host}{path}"), host, path)
        }
    }

    /// A signed request; returns the response status and body reader.
    fn call(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<(u16, ureq::http::Response<ureq::Body>), DestError> {
        let (mut url, host, path) = self.target(key);
        let mut pairs: Vec<(String, String)> = query
            .iter()
            .map(|(k, v)| (encode(k, false), encode(v, false)))
            .collect();
        pairs.sort();
        let query_text = pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        if !query_text.is_empty() {
            url.push('?');
            url.push_str(&query_text);
        }
        let payload = sha_hex(body.unwrap_or(&[]));
        let date = amz_now();
        let auth = authorization(
            &Signed {
                method,
                host: &host,
                path: &path,
                query: &query_text,
                payload_hash: &payload,
                amz_date: &date,
                extra: &[],
            },
            &self.config.region,
            &self.config.access_key,
            &self.config.secret_key,
        );
        let unreachable = |e: ureq::Error| DestError::Unreachable(short(&e.to_string()));
        let response = match (method, body) {
            ("GET", _) => self
                .agent
                .get(&url)
                .header("x-amz-date", &date)
                .header("x-amz-content-sha256", &payload)
                .header("Authorization", &auth)
                .call(),
            ("DELETE", _) => self
                .agent
                .delete(&url)
                .header("x-amz-date", &date)
                .header("x-amz-content-sha256", &payload)
                .header("Authorization", &auth)
                .call(),
            ("PUT", Some(b)) => self
                .agent
                .put(&url)
                .header("x-amz-date", &date)
                .header("x-amz-content-sha256", &payload)
                .header("Authorization", &auth)
                .send(b),
            ("POST", Some(b)) => self
                .agent
                .post(&url)
                .header("x-amz-date", &date)
                .header("x-amz-content-sha256", &payload)
                .header("Authorization", &auth)
                .send(b),
            _ => return Err(DestError::Other("bad request".into())),
        }
        .map_err(unreachable)?;
        Ok((response.status().as_u16(), response))
    }
}

/// Refusals are said in fixed words; what the server printed is cut short and only
/// its `<Code>` is kept.
fn short(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(120).collect()
}

fn refusal(status: u16, mut response: ureq::http::Response<ureq::Body>) -> DestError {
    let body = response.body_mut().read_to_string().unwrap_or_default();
    let code = between(&body, "<Code>", "</Code>").unwrap_or("");
    let code: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(60)
        .collect();
    match status {
        404 => DestError::NotFound,
        401 | 403 => DestError::Refused(format!("the server said {status} {code}")),
        _ => DestError::Other(format!("the server said {status} {code}")),
    }
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(&text[start..end])
}

/// Reads up to `size` bytes (fewer at the end of the data).
fn fill(data: &mut dyn Read, size: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(size.min(1 << 20));
    data.take(size as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

impl Destination for S3 {
    fn put(&self, name: &str, data: &mut dyn Read) -> Result<u64, DestError> {
        check(name)?;
        let io = |e: std::io::Error| {
            DestError::Other(format!("cannot read what is to be stored: {}", e.kind()))
        };
        let first = fill(data, PART_BYTES).map_err(io)?;
        if first.len() < PART_BYTES {
            let (status, response) = self.call("PUT", name, &[], Some(&first))?;
            return if status == 200 {
                Ok(first.len() as u64)
            } else {
                Err(refusal(status, response))
            };
        }
        // Multipart.
        let (status, mut response) = self.call("POST", name, &[("uploads", "")], Some(&[]))?;
        if status != 200 {
            return Err(refusal(status, response));
        }
        let text = response.body_mut().read_to_string().unwrap_or_default();
        let upload_id = between(&text, "<UploadId>", "</UploadId>")
            .ok_or_else(|| DestError::Other("the server gave no upload id".into()))?
            .to_owned();
        let work = (|| -> Result<u64, DestError> {
            let mut total = 0u64;
            let mut etags: Vec<String> = Vec::new();
            let mut part = first;
            loop {
                if etags.len() >= MAX_PARTS {
                    return Err(DestError::Other(
                        "the object is bigger than a multipart upload can be".into(),
                    ));
                }
                let number = (etags.len() + 1).to_string();
                let (status, response) = self.call(
                    "PUT",
                    name,
                    &[("partNumber", &number), ("uploadId", &upload_id)],
                    Some(&part),
                )?;
                if status != 200 {
                    return Err(refusal(status, response));
                }
                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| DestError::Other("the server gave no etag".into()))?
                    .to_owned();
                etags.push(etag);
                total += part.len() as u64;
                let done = part.len() < PART_BYTES;
                if done {
                    break;
                }
                part = fill(data, PART_BYTES).map_err(io)?;
                if part.is_empty() {
                    break;
                }
            }
            let mut xml = String::from("<CompleteMultipartUpload>");
            for (i, etag) in etags.iter().enumerate() {
                xml.push_str(&format!(
                    "<Part><PartNumber>{}</PartNumber><ETag>{}</ETag></Part>",
                    i + 1,
                    etag.replace('&', "&amp;").replace('<', "&lt;")
                ));
            }
            xml.push_str("</CompleteMultipartUpload>");
            let (status, mut response) = self.call(
                "POST",
                name,
                &[("uploadId", &upload_id)],
                Some(xml.as_bytes()),
            )?;
            let body = response.body_mut().read_to_string().unwrap_or_default();
            // S3 may answer 200 with an error in the body.
            if status != 200 || body.contains("<Error>") {
                return Err(DestError::Other(format!(
                    "the server did not finish the upload ({status})"
                )));
            }
            Ok(total)
        })();
        if work.is_err() {
            let _ = self.call("DELETE", name, &[("uploadId", &upload_id)], None);
        }
        work
    }

    fn get(&self, name: &str) -> Result<Box<dyn Read + Send>, DestError> {
        check(name)?;
        let (status, response) = self.call("GET", name, &[], None)?;
        if status != 200 {
            return Err(refusal(status, response));
        }
        Ok(Box::new(response.into_body().into_reader()))
    }

    fn delete(&self, name: &str) -> Result<(), DestError> {
        check(name)?;
        let (status, response) = self.call("DELETE", name, &[], None)?;
        match status {
            200 | 204 | 404 => Ok(()),
            _ => Err(refusal(status, response)),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, DestError> {
        let mut out = Vec::new();
        let mut token = String::new();
        let base = if self.config.prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", self.config.prefix)
        };
        loop {
            let full_prefix = format!("{base}{prefix}");
            let mut query = vec![("list-type", "2"), ("prefix", full_prefix.as_str())];
            if !token.is_empty() {
                query.push(("continuation-token", token.as_str()));
            }
            let (status, mut response) = self.call("GET", "", &query, None)?;
            if status != 200 {
                return Err(refusal(status, response));
            }
            let text = response.body_mut().read_to_string().unwrap_or_default();
            let mut rest = text.as_str();
            while let Some(key) = between(rest, "<Key>", "</Key>") {
                if let Some(name) = key.strip_prefix(&base) {
                    out.push(name.to_owned());
                }
                let end = rest.find("</Key>").map_or(rest.len(), |i| i + 6);
                rest = &rest[end..];
            }
            if text.contains("<IsTruncated>true</IsTruncated>") {
                token = between(&text, "<NextContinuationToken>", "</NextContinuationToken>")
                    .unwrap_or("")
                    .to_owned();
                if token.is_empty() {
                    break;
                }
            } else {
                break;
            }
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The worked example in AWS's documentation (GET Object with a Range header).
    #[test]
    fn the_signature_matches_the_published_example() {
        let auth = authorization(
            &Signed {
                method: "GET",
                host: "examplebucket.s3.amazonaws.com",
                path: "/test.txt",
                query: "",
                payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                amz_date: "20130524T000000Z",
                extra: &[("range", "bytes=0-9")],
            },
            "us-east-1",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn encoding_and_the_clock_are_right() {
        assert_eq!(encode("a b/c+d~é", true), "a%20b/c%2Bd~%C3%A9");
        assert_eq!(encode("a/b", false), "a%2Fb");
        let now = amz_now();
        assert_eq!(now.len(), 16);
        assert!(
            now.starts_with("20") && now.ends_with('Z') && &now[8..9] == "T",
            "{now}"
        );
    }

    #[test]
    fn urls_follow_the_style() {
        let mut c = S3Config {
            endpoint: "https://s3.example.com".into(),
            bucket: "bk".into(),
            region: "r".into(),
            prefix: "panel".into(),
            path_style: true,
            access_key: "a".into(),
            secret_key: "b".into(),
        };
        let s = S3::new(c.clone()).unwrap();
        assert_eq!(
            s.target("b1/x.enc").0,
            "https://s3.example.com/bk/panel/b1/x.enc"
        );
        c.path_style = false;
        let s = S3::new(c).unwrap();
        assert_eq!(
            s.target("b1/x.enc").0,
            "https://bk.s3.example.com/panel/b1/x.enc"
        );
        assert!(
            S3::new(S3Config {
                endpoint: "http://evil.example.com".into(),
                bucket: "b".into(),
                region: "r".into(),
                prefix: String::new(),
                path_style: true,
                access_key: "a".into(),
                secret_key: "b".into()
            })
            .is_err()
        );
    }
}
