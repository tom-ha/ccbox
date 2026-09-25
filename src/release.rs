use std::fmt;
use std::io::Read;
use std::time::Duration;

use serde_json::Value;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_RELEASES_URL: &str = "https://api.github.com/repos/tom-ha/ccbox";
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.trim().strip_prefix('v').unwrap_or(s.trim()).split('.');
        let mut next = || -> Option<u64> {
            let p = parts.next()?;
            (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())).then(|| p.parse().ok())?
        };
        let v = Version {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        parts.next().is_none().then_some(v)
    }

    pub fn installed() -> Self {
        Self::parse(VERSION).expect("CARGO_PKG_VERSION is X.Y.Z")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

pub fn releases_url() -> String {
    std::env::var("CCBOX_RELEASES_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_RELEASES_URL.to_string())
}

pub fn triple_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
        _ => None,
    }
}

pub fn host_triple() -> Option<&'static str> {
    triple_for(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn tarball_name(version: Version, triple: &str) -> String {
    format!("ccbox-{version}-{triple}.tar.gz")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: Version,
    pub notes: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Latest,
    Pinned(Version),
}

pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .user_agent(format!("ccbox/{VERSION}"))
        .build()
        .into()
}

/// `Ok(None)`: the source has no such release (HTTP 404).
pub fn fetch_release(agent: &ureq::Agent, base: &str, which: Which) -> Result<Option<Release>, String> {
    let url = match which {
        Which::Latest => format!("{base}/releases/latest"),
        Which::Pinned(v) => format!("{base}/releases/tags/v{v}"),
    };
    let mut resp = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    match resp.status().as_u16() {
        200 => {}
        404 => return Ok(None),
        code => return Err(format!("{url}: HTTP {code}")),
    }
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("{url}: {e}"))?;
    let json: Value = serde_json::from_str(&body).map_err(|e| format!("{url}: {e}"))?;
    parse_release(&json).map(Some).map_err(|e| format!("{url}: {e}"))
}

pub fn parse_release(json: &Value) -> Result<Release, String> {
    let tag = json["tag_name"].as_str().ok_or("release has no tag_name")?;
    let version = Version::parse(tag).ok_or_else(|| format!("release tag {tag:?} is not vX.Y.Z"))?;
    let assets = json["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some(Asset {
                        name: x["name"].as_str()?.to_string(),
                        url: x["browser_download_url"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release {
        version,
        notes: json["body"].as_str().unwrap_or_default().trim().to_string(),
        assets,
    })
}

pub fn download(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let mut resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    let code = resp.status().as_u16();
    if code != 200 {
        return Err(format!("{url}: HTTP {code}"));
    }
    let mut buf = Vec::new();
    resp.body_mut()
        .as_reader()
        .take(MAX_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{url}: {e}"))?;
    if buf.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err(format!("{url}: larger than {MAX_DOWNLOAD_BYTES} bytes"));
    }
    Ok(buf)
}

pub fn parse_sha256sums(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let malformed = || format!("SHA256SUMS line {} is malformed: {line:?}", i + 1);
        let (digest, rest) = line.split_once(char::is_whitespace).ok_or_else(malformed)?;
        let name = rest.trim_start().trim_start_matches('*').trim_end();
        let hex = digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit());
        if !hex || name.is_empty() {
            return Err(malformed());
        }
        out.push((digest.to_ascii_lowercase(), name.to_string()));
    }
    Ok(out)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn verify_sha256(bytes: &[u8], sums: &str, name: &str) -> Result<(), String> {
    let expected = parse_sha256sums(sums)?
        .into_iter()
        .find(|(_, n)| n == name)
        .map(|(d, _)| d)
        .ok_or_else(|| format!("SHA256SUMS has no line for {name}"))?;
    let actual = sha256_hex(bytes);
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {name}: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn triple_mapping_covers_the_four_release_targets() {
        assert_eq!(triple_for("macos", "aarch64"), Some("aarch64-apple-darwin"));
        assert_eq!(triple_for("macos", "x86_64"), Some("x86_64-apple-darwin"));
        assert_eq!(triple_for("linux", "x86_64"), Some("x86_64-unknown-linux-musl"));
        assert_eq!(triple_for("linux", "aarch64"), Some("aarch64-unknown-linux-musl"));
        assert_eq!(triple_for("windows", "x86_64"), None);
        assert_eq!(triple_for("linux", "riscv64"), None);
        assert_eq!(triple_for("freebsd", "x86_64"), None);
    }

    #[test]
    fn version_parse_accepts_x_y_z_and_a_v_prefix_only() {
        assert_eq!(v("0.6.0"), Version { major: 0, minor: 6, patch: 0 });
        assert_eq!(v("v10.20.30"), Version { major: 10, minor: 20, patch: 30 });
        for bad in ["", "0.6", "0.6.0.1", "1.2.x", "1.2.3-rc1", "v", "1..3", "+1.2.3"] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn version_order_is_numeric_per_component() {
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(v("0.6.1") > v("0.6.0"));
        assert!(v("0.0.9") < v("0.1.0"));
        assert_eq!(v("1.2.3"), v("v1.2.3"));
        assert_eq!(v("9.9.9").to_string(), "9.9.9");
    }

    #[test]
    fn installed_version_parses() {
        assert_eq!(Version::installed().to_string(), VERSION);
    }

    #[test]
    fn sha256sums_parse_and_lookup() {
        let d = "a".repeat(64);
        let text = format!("{d}  ccbox-1.2.3-aarch64-apple-darwin.tar.gz\n\n{}  *other.tar.gz\n", "B".repeat(64));
        let parsed = parse_sha256sums(&text).unwrap();
        assert_eq!(
            parsed,
            vec![
                (d.clone(), "ccbox-1.2.3-aarch64-apple-darwin.tar.gz".to_string()),
                ("b".repeat(64), "other.tar.gz".to_string()),
            ]
        );
        assert!(parse_sha256sums("abc  x.tar.gz").is_err(), "short digest");
        assert!(parse_sha256sums(&format!("{}  x", "g".repeat(64))).is_err(), "not hex");
        assert!(parse_sha256sums(&d).is_err(), "no name");
    }

    #[test]
    fn verify_sha256_accepts_a_match_and_names_both_digests_on_mismatch() {
        let bytes = b"ccbox";
        let good = sha256_hex(bytes);
        let sums = format!("{good}  t.tar.gz\n");
        assert!(verify_sha256(bytes, &sums, "t.tar.gz").is_ok());

        let wrong = "0".repeat(64);
        let err = verify_sha256(bytes, &format!("{wrong}  t.tar.gz\n"), "t.tar.gz").unwrap_err();
        assert!(err.contains(&wrong) && err.contains(&good), "{err}");

        let err = verify_sha256(bytes, &sums, "missing.tar.gz").unwrap_err();
        assert!(err.contains("no line for missing.tar.gz"), "{err}");
    }

    #[test]
    fn sha256_hex_matches_a_known_digest() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn parse_release_reads_tag_notes_and_assets() {
        let json = serde_json::json!({
            "tag_name": "v0.6.0",
            "body": "### Added\n- things\n",
            "assets": [
                {"name": "SHA256SUMS", "browser_download_url": "https://x/SHA256SUMS"},
                {"name": "broken"}
            ]
        });
        let r = parse_release(&json).unwrap();
        assert_eq!(r.version, v("0.6.0"));
        assert_eq!(r.notes, "### Added\n- things");
        assert_eq!(r.assets.len(), 1);
        assert_eq!(r.asset("SHA256SUMS").unwrap().url, "https://x/SHA256SUMS");
        assert!(parse_release(&serde_json::json!({"tag_name": "nightly"})).is_err());
    }
}
