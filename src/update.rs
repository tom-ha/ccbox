use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::release::{self, Release, Version, Which};
use crate::setup;

const API_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Default, PartialEq)]
pub struct Args {
    pub check: bool,
    pub version: Option<Version>,
    pub force: bool,
}

pub const HELP_REQUESTED: &str = "help requested";

pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let pinned = match a.as_str() {
            "--check" => {
                out.check = true;
                continue;
            }
            "--force" => {
                out.force = true;
                continue;
            }
            "-h" | "--help" => return Err(HELP_REQUESTED.to_string()),
            "--version" => it.next().ok_or("--version needs X.Y.Z")?.as_str(),
            other => match other.strip_prefix("--version=") {
                Some(v) => v,
                None => return Err(format!("unexpected argument: {other}")),
            },
        };
        out.version = Some(
            Version::parse(pinned)
                .ok_or_else(|| format!("--version wants X.Y.Z, got {pinned:?}"))?,
        );
    }
    Ok(out)
}

pub fn run(args: &Args, out: &mut dyn Write) -> Result<(), String> {
    let installed = Version::installed();
    let base = release::releases_url();
    let api = release::agent(API_TIMEOUT);
    let which = args.version.map_or(Which::Latest, Which::Pinned);
    let found = release::fetch_release(&api, &base, which)
        .map_err(|e| format!("could not look up the release: {e}"))?;

    let Some(rel) = found else {
        return match which {
            Which::Pinned(v) => Err(format!("release {v} was not found at {base}")),
            Which::Latest => {
                say(
                    out,
                    &format!(
                        "no ccbox release has been published yet; ccbox {installed} is installed"
                    ),
                );
                if args.check {
                    Ok(())
                } else {
                    rewire_in_place(out)
                }
            }
        };
    };
    let target = rel.version;

    if args.check {
        let line = match (which, target.cmp(&installed)) {
            (_, std::cmp::Ordering::Equal) => format!("already on {installed}"),
            (_, std::cmp::Ordering::Greater) => format!("{installed} -> {target} available"),
            (Which::Latest, std::cmp::Ordering::Less) => {
                format!("already on {installed}, which is newer than the latest release {target}")
            }
            (Which::Pinned(_), std::cmp::Ordering::Less) => {
                format!("{installed} -> {target} available as a downgrade (needs --force)")
            }
        };
        say(out, &line);
        return Ok(());
    }

    match (which, target.cmp(&installed)) {
        (_, std::cmp::Ordering::Equal) => {
            say(out, &format!("already on {installed}; no update performed"));
            return rewire_in_place(out);
        }
        (Which::Latest, std::cmp::Ordering::Less) => {
            say(
                out,
                &format!("ccbox {installed} is newer than the latest release {target}; no update performed"),
            );
            return rewire_in_place(out);
        }
        (Which::Pinned(_), std::cmp::Ordering::Less) if !args.force => {
            return Err(format!(
                "{target} is older than the installed {installed}; downgrades require --force"
            ));
        }
        _ => {}
    }

    let exe = current_exe()?;
    install(&rel, &exe)?;
    say(out, &format!("{installed} -> {target}"));

    let wiring = Command::new(&exe)
        .arg("setup")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())
        .and_then(|o| {
            if o.status.success() {
                Ok(String::from_utf8_lossy(&o.stdout).into_owned())
            } else {
                Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
            }
        });
    match wiring {
        Ok(text) => {
            let _ = out.write_all(text.as_bytes());
        }
        Err(e) => {
            return Err(format!(
                "{} now runs ccbox {target}, but wiring settings.json failed:\n  {e}\nrun `ccbox update` again to retry",
                exe.display()
            ))
        }
    }
    if !rel.notes.is_empty() {
        say(out, &format!("\nWhat's new in {target}:\n{}", rel.notes));
    }
    Ok(())
}

fn say(out: &mut dyn Write, line: &str) {
    let _ = writeln!(out, "{line}");
}

fn current_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    fs::canonicalize(&exe).map_err(|e| format!("cannot resolve {}: {e}", exe.display()))
}

pub fn print_setup_report(out: &mut dyn Write, r: &setup::Report) {
    if !r.changed() {
        say(
            out,
            &format!("{} is already wired for this ccbox", r.settings.display()),
        );
        return;
    }
    say(out, &format!("updated {}:", r.settings.display()));
    for line in &r.lines {
        say(out, &format!("  {line}"));
    }
    if let Some(b) = &r.backup {
        say(
            out,
            &format!("  backed up the previous file to {}", b.display()),
        );
    }
}

fn rewire_in_place(out: &mut dyn Write) -> Result<(), String> {
    let exe = current_exe()?;
    let report = setup::run(&setup::claude_dir(), &exe)
        .map_err(|e| format!("wiring settings.json failed: {e}"))?;
    print_setup_report(out, &report);
    Ok(())
}

struct Staging {
    path: PathBuf,
    keep: bool,
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Downloads, verifies and swaps in `rel` for `exe`; `exe` is unchanged on error.
pub fn install(rel: &Release, exe: &Path) -> Result<(), String> {
    let triple = release::host_triple().ok_or_else(|| {
        format!(
            "no prebuilt ccbox for {}/{}; build from source with: cargo install --git https://github.com/tom-ha/ccbox --locked",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let dir = exe.parent().ok_or("the binary has no parent directory")?;
    let mut staging = Staging {
        path: dir.join(format!(".ccbox-update-{}.tmp", std::process::id())),
        keep: false,
    };
    let mut file = create_staging(&staging.path).map_err(|e| {
        format!(
            "cannot write to {} ({e}), so {} cannot be replaced. Re-run install.sh with CCBOX_BIN_DIR set to a directory you can write to, or update it with elevated privileges and then run `ccbox update` as yourself to rewire your settings.json",
            dir.display(),
            exe.display()
        )
    })?;

    let name = release::tarball_name(rel.version, triple);
    let sums_asset = rel.asset("SHA256SUMS").ok_or_else(|| {
        format!(
            "release {} has no SHA256SUMS; refusing to install it",
            rel.version
        )
    })?;
    let tar_asset = rel
        .asset(&name)
        .ok_or_else(|| format!("release {} has no {name}", rel.version))?;
    let dl = release::agent(DOWNLOAD_TIMEOUT);
    let sums = release::download(&dl, &sums_asset.url)?;
    let sums = String::from_utf8(sums).map_err(|_| "SHA256SUMS is not text".to_string())?;
    let tarball = release::download(&dl, &tar_asset.url)?;
    release::verify_sha256(&tarball, &sums, &name)?;

    extract_ccbox(&tarball, &mut file).map_err(|e| format!("cannot unpack {name}: {e}"))?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    probe_version(&staging.path, rel.version)?;
    fs::rename(&staging.path, exe).map_err(|e| format!("cannot replace {}: {e}", exe.display()))?;
    staging.keep = true;
    Ok(())
}

fn create_staging(path: &Path) -> io::Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o755);
    }
    let f = opts.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(fs::Permissions::from_mode(0o755))?;
    }
    Ok(f)
}

fn extract_ccbox(tarball: &[u8], dest: &mut fs::File) -> io::Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tarball));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let is_ccbox = path
            .components()
            .map(|c| c.as_os_str())
            .eq([std::ffi::OsStr::new("ccbox")])
            || path == Path::new("./ccbox");
        if is_ccbox && entry.header().entry_type().is_file() {
            io::copy(&mut entry, dest)?;
            return Ok(());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no ccbox binary in the archive",
    ))
}

/// Runs the staged binary's `version` so a binary that cannot run here never replaces this one.
fn probe_version(path: &Path, want: Version) -> Result<(), String> {
    use wait_timeout::ChildExt;
    let mut child = Command::new(path)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("the downloaded ccbox does not run on this machine: {e}"))?;
    let status = child
        .wait_timeout(PROBE_TIMEOUT)
        .map_err(|e| e.to_string())?;
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the downloaded ccbox did not answer `ccbox version`".to_string());
    }
    let mut stdout = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut stdout);
    }
    let want_line = format!("ccbox {want}");
    if stdout.trim() != want_line {
        return Err(format!(
            "the downloaded binary reports {:?}, expected {want_line:?}",
            stdout.trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_args_reads_the_three_flags() {
        assert_eq!(parse_args(&[]).unwrap(), Args::default());
        let got = parse_args(&a(&["--check", "--version", "0.6.0", "--force"])).unwrap();
        assert!(got.check && got.force);
        assert_eq!(got.version, Version::parse("0.6.0"));
        assert_eq!(
            parse_args(&a(&["--version=v1.2.3"])).unwrap().version,
            Version::parse("1.2.3")
        );
        assert!(parse_args(&a(&["--version"])).is_err());
        assert!(parse_args(&a(&["--version", "0.6"])).is_err());
        assert!(parse_args(&a(&["--yes"])).is_err());
    }

    fn tarball(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for (name, body) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o755);
            h.set_cksum();
            b.append_data(&mut h, name, *body).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn extract_takes_only_the_top_level_ccbox() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("out");
        let t = tarball(&[("sub/ccbox", b"no"), ("ccbox", b"yes")]);
        extract_ccbox(&t, &mut fs::File::create(&p).unwrap()).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"yes");
        let t = tarball(&[("ccbox-demo", b"no")]);
        assert!(extract_ccbox(&t, &mut fs::File::create(&p).unwrap()).is_err());
    }

    #[test]
    fn staging_file_is_removed_unless_kept() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".ccbox-update.tmp");
        {
            let _s = Staging {
                path: p.clone(),
                keep: false,
            };
            create_staging(&p).unwrap();
        }
        assert!(!p.exists());
        {
            let _s = Staging {
                path: p.clone(),
                keep: true,
            };
            create_staging(&p).unwrap();
        }
        assert!(p.exists());
    }
}
