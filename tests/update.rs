use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

const INSTALLED: &str = env!("CARGO_PKG_VERSION");

type Routes = Arc<Mutex<HashMap<String, (u16, Vec<u8>)>>>;

struct FakeReleases {
    base: String,
    routes: Routes,
    hits: Arc<Mutex<Vec<String>>>,
}

impl FakeReleases {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/repos/o/ccbox", listener.local_addr().unwrap());
        let routes: Routes = Arc::default();
        let hits: Arc<Mutex<Vec<String>>> = Arc::default();
        let (r, h) = (routes.clone(), hits.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                        break;
                    }
                }
                h.lock().unwrap().push(path.clone());
                let (status, body) = r
                    .lock()
                    .unwrap()
                    .get(&path)
                    .cloned()
                    .unwrap_or((404, b"{}".to_vec()));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        let fake = FakeReleases { base, routes, hits };
        fake.route(&fake.path(""), 200, "{}");
        fake
    }

    fn route(&self, path: &str, status: u16, body: impl Into<Vec<u8>>) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.to_string(), (status, body.into()));
    }

    fn url(&self, name: &str) -> String {
        format!("{}/download/{name}", self.base)
    }

    fn path(&self, suffix: &str) -> String {
        let root = self.base.splitn(4, '/').nth(3).unwrap();
        format!("/{root}{suffix}")
    }

    fn publish(&self, version: &str, binary: &[u8], corrupt_sums: bool) {
        let triple = ccbox::release::host_triple().expect("a release target");
        let name = format!("ccbox-{version}-{triple}.tar.gz");
        let tarball = tarball(binary);
        let digest = if corrupt_sums {
            "0".repeat(64)
        } else {
            ccbox::release::sha256_hex(&tarball)
        };
        let release = serde_json::json!({
            "tag_name": format!("v{version}"),
            "body": "### Added\n- a test release",
            "assets": [
                {"name": name, "browser_download_url": self.url(&name)},
                {"name": "SHA256SUMS", "browser_download_url": self.url("SHA256SUMS")},
            ]
        })
        .to_string();
        self.route(&self.path("/releases/latest"), 200, release.clone());
        self.route(
            &self.path(&format!("/releases/tags/v{version}")),
            200,
            release,
        );
        self.route(&self.path(&format!("/download/{name}")), 200, tarball);
        self.route(
            &self.path("/download/SHA256SUMS"),
            200,
            format!("{digest}  {name}\n"),
        );
    }
}

fn tarball(binary: &[u8]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut b = tar::Builder::new(gz);
    let mut h = tar::Header::new_gnu();
    h.set_size(binary.len() as u64);
    h.set_mode(0o755);
    h.set_cksum();
    b.append_data(&mut h, "ccbox", binary).unwrap();
    b.into_inner().unwrap().finish().unwrap()
}

fn fake_binary(version: &str) -> Vec<u8> {
    format!("#!/bin/sh\ncase \"$1\" in\n  version) echo \"ccbox {version}\" ;;\n  setup) echo \"setup ran for {version}\" ;;\nesac\n")
        .into_bytes()
}

fn bump_major(v: &str) -> String {
    let major: u64 = v.split('.').next().unwrap().parse().unwrap();
    format!("{}.0.0", major + 1)
}

struct Install {
    _dir: tempfile::TempDir,
    exe: PathBuf,
    claude: PathBuf,
}

impl Install {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let claude = dir.path().join("claude");
        fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("ccbox");
        fs::copy(env!("CARGO_BIN_EXE_ccbox"), &exe).unwrap();
        Install {
            _dir: dir,
            exe,
            claude,
        }
    }

    fn run(&self, fake: &FakeReleases, args: &[&str]) -> Output {
        Command::new(&self.exe)
            .args(args)
            .env("CCBOX_RELEASES_URL", &fake.base)
            .env("CLAUDE_CONFIG_DIR", &self.claude)
            .output()
            .unwrap()
    }

    fn staging_files(&self) -> Vec<String> {
        fs::read_dir(self.exe.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n != "ccbox")
            .collect()
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn sha(p: &Path) -> String {
    ccbox::release::sha256_hex(&fs::read(p).unwrap())
}

#[test]
fn update_replaces_the_binary_and_runs_the_new_setup() {
    let fake = FakeReleases::start();
    let next = bump_major(INSTALLED);
    let binary = fake_binary(&next);
    fake.publish(&next, &binary, false);
    let inst = Install::new();

    let out = inst.run(&fake, &["update"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains(&format!("{INSTALLED} -> {next}")), "{t}");
    assert!(t.contains(&format!("setup ran for {next}")), "{t}");
    assert!(t.contains("a test release"), "{t}");
    assert_eq!(fs::read(&inst.exe).unwrap(), binary);
    assert!(
        inst.staging_files().is_empty(),
        "{:?}",
        inst.staging_files()
    );
}

#[test]
fn a_checksum_mismatch_leaves_the_binary_alone() {
    let fake = FakeReleases::start();
    let next = bump_major(INSTALLED);
    fake.publish(&next, &fake_binary(&next), true);
    let inst = Install::new();
    let before = sha(&inst.exe);

    let out = inst.run(&fake, &["update"]);
    let t = text(&out);
    assert!(!out.status.success(), "{t}");
    assert!(t.contains("checksum mismatch"), "{t}");
    assert_eq!(sha(&inst.exe), before);
    assert!(
        inst.staging_files().is_empty(),
        "{:?}",
        inst.staging_files()
    );
}

#[test]
fn check_reports_and_writes_nothing() {
    let fake = FakeReleases::start();
    let next = bump_major(INSTALLED);
    fake.publish(&next, &fake_binary(&next), false);
    let inst = Install::new();
    let before = sha(&inst.exe);

    let out = inst.run(&fake, &["update", "--check"]);
    assert!(out.status.success());
    assert_eq!(
        text(&out).trim(),
        format!("{INSTALLED} -> {next} available")
    );
    assert_eq!(sha(&inst.exe), before);
    assert!(!inst.claude.exists(), "--check wrote the config dir");
    assert!(!fake
        .hits
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("/download/")));
}

#[test]
fn already_current_rewires_settings_and_keeps_the_binary() {
    let fake = FakeReleases::start();
    fake.publish(INSTALLED, b"never downloaded", false);
    let inst = Install::new();
    let before = sha(&inst.exe);

    let out = inst.run(&fake, &["update"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains(&format!("already on {INSTALLED}; no update performed")),
        "{t}"
    );
    assert_eq!(sha(&inst.exe), before);
    let settings = fs::read_to_string(inst.claude.join("settings.json")).unwrap();
    assert!(settings.contains("\"refreshInterval\": 5"), "{settings}");

    let again = text(&inst.run(&fake, &["update"]));
    assert!(again.contains("already wired"), "{again}");
}

#[test]
fn no_release_yet_is_not_a_failure() {
    let fake = FakeReleases::start();
    let inst = Install::new();
    let out = inst.run(&fake, &["update", "--check"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("no ccbox release has been published yet"), "{t}");
}

#[test]
fn downgrade_needs_force_and_a_missing_version_is_not_found() {
    let fake = FakeReleases::start();
    let old = "0.0.1";
    fake.publish(old, &fake_binary(old), false);
    let inst = Install::new();
    let before = sha(&inst.exe);

    let out = inst.run(&fake, &["update", "--version", old]);
    assert!(!out.status.success());
    assert!(
        text(&out).contains("downgrades require --force"),
        "{}",
        text(&out)
    );
    assert_eq!(sha(&inst.exe), before);

    let out = inst.run(&fake, &["update", "--version", "9.9.9"]);
    assert!(!out.status.success());
    assert!(
        text(&out).contains("release 9.9.9 was not found"),
        "{}",
        text(&out)
    );

    let out = inst.run(&fake, &["update", "--version", old, "--force"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(fs::read(&inst.exe).unwrap(), fake_binary(old));
}

#[test]
fn a_missing_repository_is_an_error_not_no_release() {
    let fake = FakeReleases::start();
    fake.route(&fake.path(""), 404, "{}");
    let inst = Install::new();
    let out = inst.run(&fake, &["update", "--check"]);
    let t = text(&out);
    assert!(!out.status.success(), "{t}");
    assert!(t.contains("no such repository"), "{t}");
}

#[test]
fn an_empty_claude_config_dir_means_home_dot_claude() {
    let inst = Install::new();
    let home = inst.exe.parent().unwrap().parent().unwrap().join("home");
    let cwd = inst.exe.parent().unwrap().parent().unwrap().join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    let out = Command::new(&inst.exe)
        .arg("setup")
        .env("CLAUDE_CONFIG_DIR", "")
        .env("HOME", &home)
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    assert!(home.join(".claude/settings.json").exists());
    assert_eq!(fs::read_dir(&cwd).unwrap().count(), 0);
}
