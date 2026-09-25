use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

pub const HOOK_EVENTS: &[(&str, &[&str])] = &[
    ("Notification", &[""]),
    ("PermissionRequest", &[""]),
    ("PermissionDenied", &[""]),
    ("PreToolUse", &["AskUserQuestion"]),
    ("PostToolUse", &[""]),
    ("PostToolUseFailure", &[""]),
    ("UserPromptSubmit", &[""]),
    ("Stop", &[""]),
    ("SubagentStop", &[""]),
    ("SessionEnd", &[""]),
];

pub const REFRESH_INTERVAL: u64 = 5;

/// Python's `shlex.quote`.
pub fn shlex_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if s.chars().all(safe) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// Python's `shlex.split` (POSIX mode, no comments); `None` where it raises.
pub fn shlex_split(s: &str) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum St {
        Space,
        Word,
        Quote(char),
        Escape(Option<char>),
    }
    let is_space = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');
    let mut out = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut state = St::Space;
    for c in s.chars() {
        match state {
            St::Space | St::Word if is_space(c) => {
                if state == St::Word || quoted {
                    out.push(std::mem::take(&mut token));
                }
                quoted = false;
                state = St::Space;
            }
            St::Space | St::Word if c == '\\' => state = St::Escape(None),
            St::Space | St::Word if c == '\'' || c == '"' => state = St::Quote(c),
            St::Space | St::Word => {
                token.push(c);
                state = St::Word;
            }
            St::Quote(q) => {
                quoted = true;
                if c == q {
                    state = St::Word;
                } else if q == '"' && c == '\\' {
                    state = St::Escape(Some(q));
                } else {
                    token.push(c);
                }
            }
            St::Escape(inside) => {
                if let Some(q) = inside {
                    if c != '\\' && c != q {
                        token.push('\\');
                    }
                }
                token.push(c);
                state = inside.map_or(St::Word, St::Quote);
            }
        }
    }
    match state {
        St::Quote(_) | St::Escape(_) => None,
        St::Word => {
            out.push(token);
            Some(out)
        }
        St::Space => {
            if quoted {
                out.push(token);
            }
            Some(out)
        }
    }
}

/// `PurePosixPath(p).name`.
fn posix_name(p: &str) -> &str {
    p.split('/')
        .rfind(|part| !part.is_empty() && *part != ".")
        .unwrap_or("")
}

pub fn is_ccbox_hook(h: &Value) -> bool {
    let Some(cmd) = h
        .as_object()
        .and_then(|o| o.get("command"))
        .and_then(Value::as_str)
    else {
        return false;
    };
    match shlex_split(cmd).as_deref() {
        Some([.., bin, last]) => last == "hook" && posix_name(bin) == "ccbox",
        _ => false,
    }
}

/// One line per change; `data` is untouched on error.
pub fn wire(data: &mut Value, ccbox: &str) -> Result<Vec<String>, String> {
    let obj = data
        .as_object_mut()
        .ok_or("settings.json is not a JSON object; not touching it")?;
    if obj.get("hooks").is_some_and(|h| !h.is_object()) {
        return Err("settings.json 'hooks' is not an object; not touching it".to_string());
    }
    let mut lines = Vec::new();
    let quoted = shlex_quote(ccbox);

    let status_line =
        json!({"type": "command", "command": quoted, "refreshInterval": REFRESH_INTERVAL});
    match obj.insert("statusLine".to_string(), status_line.clone()) {
        Some(prev) if prev == status_line => {}
        Some(prev) => lines.push(format!("set statusLine to {quoted} (was {prev})")),
        None => lines.push(format!("set statusLine to {quoted}")),
    }

    let hook_cmd = format!("{quoted} hook");
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("checked above");
    for (event, matchers) in HOOK_EVENTS {
        let existing = match hooks.get(*event) {
            None => Vec::new(),
            Some(Value::Array(groups)) => groups.clone(),
            Some(_) => {
                lines.push(format!("left hooks.{event} alone: it is not a list"));
                continue;
            }
        };
        let mut had_ccbox = false;
        let mut groups = Vec::new();
        for g in existing {
            let inner = g
                .get("hooks")
                .and_then(Value::as_array)
                .filter(|i| !i.is_empty());
            let Some(inner) = inner else {
                groups.push(g);
                continue;
            };
            let kept: Vec<Value> = inner
                .iter()
                .filter(|h| !is_ccbox_hook(h))
                .cloned()
                .collect();
            had_ccbox |= kept.len() < inner.len();
            if !kept.is_empty() {
                let mut g = g.as_object().expect("has hooks").clone();
                g.insert("hooks".to_string(), Value::Array(kept));
                groups.push(Value::Object(g));
            }
        }
        for m in *matchers {
            groups.push(json!({"matcher": m, "hooks": [{"type": "command", "command": hook_cmd}]}));
        }
        let groups = Value::Array(groups);
        if hooks.get(*event) != Some(&groups) {
            lines.push(if had_ccbox {
                format!("updated the ccbox hook in hooks.{event}")
            } else {
                format!("added the ccbox hook to hooks.{event}")
            });
            hooks.insert(event.to_string(), groups);
        }
    }
    Ok(lines)
}

/// Python's `json.dumps(value, indent=2) + "\n"`, byte for byte.
pub fn to_python_json(value: &Value) -> String {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, PyFormatter::default());
    serde::Serialize::serialize(value, &mut ser).expect("Value serializes");
    let mut s = String::from_utf8(buf).expect("ASCII output");
    s.push('\n');
    s
}

#[derive(Default)]
struct PyFormatter<'a>(serde_json::ser::PrettyFormatter<'a>);

macro_rules! delegate {
    ($($name:ident $(($arg:ident: $ty:ty))?),* $(,)?) => {$(
        fn $name<W: ?Sized + Write>(&mut self, w: &mut W $(, $arg: $ty)?) -> io::Result<()> {
            self.0.$name(w $(, $arg)?)
        }
    )*};
}

impl serde_json::ser::Formatter for PyFormatter<'_> {
    delegate!(
        begin_array, end_array, begin_array_value(first: bool), end_array_value,
        begin_object, end_object, begin_object_key(first: bool), begin_object_value,
        end_object_value,
    );

    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        w: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        for c in fragment.chars() {
            if (' '..='~').contains(&c) {
                w.write_all(&[c as u8])?;
            } else {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    write!(w, "\\u{u:04x}")?;
                }
            }
        }
        Ok(())
    }

    fn write_f64<W: ?Sized + Write>(&mut self, w: &mut W, value: f64) -> io::Result<()> {
        w.write_all(python_float_repr(value).as_bytes())
    }
}

fn python_float_repr(f: f64) -> String {
    let sci = format!("{:e}", f.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let decpt = exp.parse::<i32>().expect("integer exponent") + 1;
    let sign = if f.is_sign_negative() { "-" } else { "" };
    let n = digits.len() as i32;
    let body = if (-3..=16).contains(&decpt) {
        if decpt <= 0 {
            format!("0.{}{digits}", "0".repeat((-decpt) as usize))
        } else if decpt >= n {
            format!("{digits}{}.0", "0".repeat((decpt - n) as usize))
        } else {
            format!(
                "{}.{}",
                &digits[..decpt as usize],
                &digits[decpt as usize..]
            )
        }
    } else {
        let frac = if n > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        let e = decpt - 1;
        format!(
            "{}{frac}e{}{:02}",
            &digits[..1],
            if e < 0 { '-' } else { '+' },
            e.abs()
        )
    };
    format!("{sign}{body}")
}

#[derive(Debug)]
pub struct Report {
    pub settings: PathBuf,
    pub lines: Vec<String>,
    pub backup: Option<PathBuf>,
}

impl Report {
    pub fn changed(&self) -> bool {
        !self.lines.is_empty() || self.backup.is_some()
    }
}

pub fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".claude")
        })
}

/// Writes only on change: a timestamped backup, then temp + rename.
pub fn run(claude_dir: &Path, ccbox: &Path) -> Result<Report, String> {
    let settings = claude_dir.join("settings.json");
    let target = fs::canonicalize(&settings).unwrap_or_else(|_| settings.clone());
    let (text, existed) = match fs::read_to_string(&target) {
        Ok(t) => (t, true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (String::new(), false),
        Err(e) => return Err(format!("cannot read {}: {e}", settings.display())),
    };
    let mut data: Value = if text.is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(&text)
            .map_err(|e| format!("cannot parse {}: {e}; not touching it", settings.display()))?
    };
    let before = data.clone();
    let ccbox = ccbox.to_str().ok_or("the ccbox path is not valid UTF-8")?;
    let lines = wire(&mut data, ccbox)?;
    let mut report = Report {
        settings: settings.clone(),
        lines,
        backup: None,
    };
    if existed && data == before {
        report.lines.clear();
        return Ok(report);
    }

    let dir = target.parent().unwrap_or(claude_dir);
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    if existed {
        let backup = backup_path(&target);
        fs::copy(&target, &backup).map_err(|e| {
            format!(
                "cannot back up {} to {}: {e}",
                settings.display(),
                backup.display()
            )
        })?;
        report.backup = Some(backup);
    }
    write_atomic(&target, to_python_json(&data).as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", settings.display()))?;
    Ok(report)
}

fn backup_path(target: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let base = format!("{}.bak.{stamp}", target.display());
    (0..)
        .map(|n| {
            PathBuf::from(if n == 0 {
                base.clone()
            } else {
                format!("{base}-{n}")
            })
        })
        .find(|p| !p.exists())
        .expect("an unused backup name")
}

fn write_atomic(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("settings.json");
    let tmp = target.with_file_name(format!(".{name}.ccbox-{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        if let Ok(meta) = fs::metadata(target) {
            f.set_permissions(meta.permissions())?;
        }
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wired(before: Value, ccbox: &str) -> (Value, Vec<String>) {
        let mut v = before;
        let lines = wire(&mut v, ccbox).unwrap();
        (v, lines)
    }

    #[test]
    fn shlex_quote_matches_python() {
        assert_eq!(shlex_quote(""), "''");
        assert_eq!(
            shlex_quote("/Users/me/.cargo/bin/ccbox"),
            "/Users/me/.cargo/bin/ccbox"
        );
        assert_eq!(shlex_quote("/a b/ccbox"), "'/a b/ccbox'");
        assert_eq!(shlex_quote("it's"), "'it'\"'\"'s'");
        assert_eq!(shlex_quote("é"), "'é'");
    }

    #[test]
    fn shlex_split_matches_python() {
        let s = |x: &str| shlex_split(x).map(|v| v.join("|"));
        assert_eq!(s("a b  c").as_deref(), Some("a|b|c"));
        assert_eq!(s("'/a b/ccbox' hook").as_deref(), Some("/a b/ccbox|hook"));
        assert_eq!(s(r#""a\"b" 'c\d'"#).as_deref(), Some(r#"a"b|c\d"#));
        assert_eq!(s(r#""a\xb""#).as_deref(), Some(r"a\xb"));
        assert_eq!(s(r"a\ b").as_deref(), Some("a b"));
        assert_eq!(s("x '' y").as_deref(), Some("x||y"));
        assert_eq!(s("a'b'c").as_deref(), Some("abc"));
        assert_eq!(s("").as_deref(), Some(""));
        assert_eq!(shlex_split("  ").unwrap().len(), 0);
        assert_eq!(s("'open"), None);
        assert_eq!(s("trailing\\"), None);
    }

    #[test]
    fn is_ccbox_hook_matches_the_installed_command_forms() {
        let h = |c: &str| json!({"type": "command", "command": c});
        assert!(is_ccbox_hook(&h("/Users/me/.cargo/bin/ccbox hook")));
        assert!(is_ccbox_hook(&h("'/a b/ccbox' hook")));
        assert!(is_ccbox_hook(&h("ccbox hook")));
        assert!(is_ccbox_hook(&h("env X=1 ccbox/ hook")));
        assert!(!is_ccbox_hook(&h("ccbox")));
        assert!(!is_ccbox_hook(&h("other-tool hook")));
        assert!(!is_ccbox_hook(&h("ccbox hook --extra")));
        assert!(!is_ccbox_hook(&h("'ccbox hook")));
        assert!(!is_ccbox_hook(&json!({"command": 5})));
        assert!(!is_ccbox_hook(&json!("ccbox hook")));
    }

    #[test]
    fn wire_adds_status_line_and_every_hook_to_an_empty_object() {
        let (v, lines) = wired(json!({}), "/x/ccbox");
        assert_eq!(
            v["statusLine"],
            json!({"type": "command", "command": "/x/ccbox", "refreshInterval": 5})
        );
        for (event, matchers) in HOOK_EVENTS {
            let groups = v["hooks"][event].as_array().unwrap();
            assert_eq!(groups.len(), matchers.len(), "{event}");
            assert_eq!(groups[0]["hooks"][0]["command"], "/x/ccbox hook");
        }
        assert_eq!(lines.len(), 1 + HOOK_EVENTS.len());
    }

    #[test]
    fn wire_is_idempotent_and_reports_nothing_the_second_time() {
        let (v, _) = wired(json!({"a": 1}), "/x/ccbox");
        let (again, lines) = wired(v.clone(), "/x/ccbox");
        assert_eq!(again, v);
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn wire_keeps_foreign_hooks_and_key_order() {
        let before = json!({
            "z": 1,
            "hooks": {
                "Stop": [
                    {"matcher": "", "hooks": [
                        {"type": "command", "command": "other hook"},
                        {"type": "command", "command": "/old/ccbox hook"}
                    ]},
                    {"hooks": [{"type": "command", "command": "/old/ccbox hook"}]}
                ]
            },
            "a": 2
        });
        let (v, lines) = wired(before, "/new/ccbox");
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["z", "hooks", "a", "statusLine"]);
        assert_eq!(
            v["hooks"]["Stop"],
            json!([
                {"matcher": "", "hooks": [{"type": "command", "command": "other hook"}]},
                {"matcher": "", "hooks": [{"type": "command", "command": "/new/ccbox hook"}]}
            ])
        );
        assert!(lines.contains(&"updated the ccbox hook in hooks.Stop".to_string()));
        assert!(lines.contains(&"added the ccbox hook to hooks.Notification".to_string()));
    }

    #[test]
    fn wire_leaves_non_list_events_and_refuses_non_object_hooks() {
        let (v, lines) = wired(json!({"hooks": {"Stop": "x"}}), "/x/ccbox");
        assert_eq!(v["hooks"]["Stop"], "x");
        assert!(lines.contains(&"left hooks.Stop alone: it is not a list".to_string()));

        let mut v = json!({"hooks": []});
        assert!(wire(&mut v, "/x/ccbox").is_err());
        assert_eq!(v, json!({"hooks": []}), "untouched on error");
        assert!(wire(&mut json!([1]), "/x/ccbox").is_err());
    }

    #[test]
    fn python_json_escapes_non_ascii_and_formats_like_json_dumps() {
        let v: Value =
            serde_json::from_str(r#"{"a": "é😀\u007f", "b": [], "c": {}, "d": [1, 2.5]}"#).unwrap();
        assert_eq!(
            to_python_json(&v),
            "{\n  \"a\": \"\\u00e9\\ud83d\\ude00\\u007f\",\n  \"b\": [],\n  \"c\": {},\n  \"d\": [\n    1,\n    2.5\n  ]\n}\n"
        );
    }

    #[test]
    fn python_float_repr_matches_python() {
        for (f, want) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (5.0, "5.0"),
            (0.1, "0.1"),
            (1e-4, "0.0001"),
            (1e-5, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1.2345678901234568e20, "1.2345678901234568e+20"),
            (123.456, "123.456"),
            (-2.5e300, "-2.5e+300"),
        ] {
            assert_eq!(python_float_repr(f), want, "{f:e}");
        }
    }

    #[test]
    fn run_writes_only_on_change_and_backs_up_first() {
        let d = tempfile::tempdir().unwrap();
        let settings = d.path().join("settings.json");
        fs::write(&settings, "{\"keep\": true}").unwrap();
        let r = run(d.path(), Path::new("/x/ccbox")).unwrap();
        let backup = r.backup.clone().unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), "{\"keep\": true}");
        let first = fs::read(&settings).unwrap();

        let r = run(d.path(), Path::new("/x/ccbox")).unwrap();
        assert!(!r.changed(), "{r:?}");
        assert_eq!(fs::read(&settings).unwrap(), first);
        let backups = fs::read_dir(d.path()).unwrap().filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".bak.")
        });
        assert_eq!(backups.count(), 1);
    }

    #[test]
    fn run_creates_a_missing_file_without_a_backup() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("new");
        let r = run(&dir, Path::new("/x/ccbox")).unwrap();
        assert!(r.backup.is_none());
        let v: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(v["statusLine"]["command"], "/x/ccbox");
    }

    #[test]
    fn run_refuses_unparseable_json_without_writing() {
        let d = tempfile::tempdir().unwrap();
        let settings = d.path().join("settings.json");
        fs::write(&settings, "{not json").unwrap();
        assert!(run(d.path(), Path::new("/x/ccbox")).is_err());
        assert_eq!(fs::read_to_string(&settings).unwrap(), "{not json");
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
    }
}
