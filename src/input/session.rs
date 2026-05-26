//! `SessionInfo` — typed view of the Claude Code session JSON blob.
//!
//! Every field uses `#[serde(default)]` so missing keys deserialize as
//! type defaults rather than failing. `Model` accepts either a bare string
//! (`"claude-sonnet-4-6"`) or an object (`{"id": "...", "display_name": "..."}`).

use serde::{Deserialize, Deserializer, Serialize};

/// Deserialize a field that may legally be `null` by substituting `T::default()`.
/// `#[serde(default)]` alone only covers *missing* keys; Claude Code sends
/// `null` for unset nested objects on the very first statusline call of a
/// fresh session, which would otherwise abort the whole parse.
fn null_to_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    let opt: Option<T> = Option::deserialize(d)?;
    Ok(opt.unwrap_or_default())
}

// ---------- Model (string-or-object) ----------------------------------------

#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct Model {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: String,
}

impl<'de> Deserialize<'de> for Model {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ModelRepr {
            Bare(String),
            Object {
                #[serde(default)]
                id: String,
                #[serde(default)]
                display_name: String,
            },
            Null,
        }
        Ok(match ModelRepr::deserialize(d)? {
            ModelRepr::Bare(s) => Model { id: s, display_name: String::new() },
            ModelRepr::Object { id, display_name } => Model { id, display_name },
            ModelRepr::Null => Model::default(),
        })
    }
}

// ---------- Small named-tuple-like types ------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputStyle {
    #[serde(default = "default_output_style_name", deserialize_with = "null_to_default")]
    pub name: String,
}
fn default_output_style_name() -> String { "default".to_string() }
impl Default for OutputStyle {
    fn default() -> Self {
        Self { name: default_output_style_name() }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Effort {
    #[serde(default, deserialize_with = "null_to_default")]
    pub level: String,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Thinking {
    #[serde(default, deserialize_with = "null_to_default")]
    pub enabled: bool,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrentUsage {
    #[serde(default, deserialize_with = "null_to_default")]
    pub input_tokens: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub output_tokens: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub cache_creation_input_tokens: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct RateBucket {
    #[serde(default, deserialize_with = "null_to_default")]
    pub used_percentage: f64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub resets_at: i64,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct RateLimits {
    #[serde(default, deserialize_with = "null_to_default")]
    pub five_hour: RateBucket,
    #[serde(default, deserialize_with = "null_to_default")]
    pub seven_day: RateBucket,
}

// ---------- Larger value types ----------------------------------------------

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(default, deserialize_with = "null_to_default")]
    pub current_dir: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub project_dir: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub added_dirs: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Cost {
    #[serde(default, deserialize_with = "null_to_default")]
    pub total_cost_usd: f64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub total_duration_ms: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub total_api_duration_ms: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub total_lines_added: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub total_lines_removed: u64,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct ContextWindow {
    #[serde(default, deserialize_with = "null_to_default")]
    pub total_input_tokens: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub total_output_tokens: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub context_window_size: u64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub current_usage: CurrentUsage,
    #[serde(default)]
    pub used_percentage: Option<f64>,
    #[serde(default)]
    pub remaining_percentage: Option<f64>,
}

// ---------- Top-level SessionInfo -------------------------------------------

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    #[serde(default, deserialize_with = "null_to_default")]
    pub session_id: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub transcript_path: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub cwd: String,
    #[serde(default)]
    pub model: Model,
    #[serde(default, deserialize_with = "null_to_default")]
    pub workspace: Workspace,
    #[serde(default, deserialize_with = "null_to_default")]
    pub version: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub output_style: OutputStyle,
    #[serde(default, deserialize_with = "null_to_default")]
    pub cost: Cost,
    #[serde(default, deserialize_with = "null_to_default")]
    pub context_window: ContextWindow,
    #[serde(default, deserialize_with = "null_to_default")]
    pub exceeds_200k_tokens: bool,
    #[serde(default, deserialize_with = "null_to_default")]
    pub effort: Effort,
    #[serde(default, deserialize_with = "null_to_default")]
    pub thinking: Thinking,
    #[serde(default, deserialize_with = "null_to_default")]
    pub fast_mode: bool,
    #[serde(default, deserialize_with = "null_to_default")]
    pub rate_limits: RateLimits,
}

impl SessionInfo {
    /// `display_name` if present, otherwise `id`, otherwise `"unknown"`. Strips
    /// `(1M context)` to `1M` and collapses double spaces.
    pub fn model_name(&self) -> String {
        let raw = if !self.model.display_name.is_empty() {
            self.model.display_name.as_str()
        } else if !self.model.id.is_empty() {
            self.model.id.as_str()
        } else {
            "unknown"
        };
        raw.replace("(1M context)", "1M").replace("  ", " ").trim().to_string()
    }

    /// Compact thinking marker (`""`, `"fast"`, `"high"`, `"high/fast"`, …).
    pub fn model_thinking(&self) -> String {
        if self.thinking.enabled && !self.effort.level.is_empty() {
            return if self.fast_mode {
                format!("{}/fast", self.effort.level)
            } else {
                self.effort.level.clone()
            };
        }
        if self.fast_mode {
            return "fast".to_string();
        }
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> SessionInfo {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn empty_object_yields_default_session_info() {
        let s: SessionInfo = serde_json::from_str("{}").unwrap();
        assert_eq!(s.session_id, "");
        assert_eq!(s.model.id, "");
        assert_eq!(s.workspace.current_dir, "");
        assert_eq!(s.output_style.name, "default");
        assert_eq!(s.exceeds_200k_tokens, false);
        assert_eq!(s.rate_limits.five_hour.used_percentage, 0.0);
    }

    #[test]
    fn fixture_round_trip() {
        let raw = std::fs::read_to_string("tests/fixtures/session-info-example.json").unwrap();
        let parsed: SessionInfo = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.model.id, "claude-sonnet-4-6");
        assert_eq!(parsed.model.display_name, "Sonnet 4.6");
        assert_eq!(parsed.workspace.current_dir, "/home/user/my-project");
        assert_eq!(parsed.cost.total_duration_ms, 807557);
        assert_eq!(parsed.context_window.context_window_size, 200_000);
        assert_eq!(parsed.context_window.current_usage.cache_read_input_tokens, 16476);
        assert_eq!(parsed.rate_limits.five_hour.used_percentage, 61.0);
        assert_eq!(parsed.rate_limits.seven_day.resets_at, 1_777_035_600);

        // Re-serialize and ensure it still deserializes back equivalently.
        let json = serde_json::to_string(&parsed).unwrap();
        let again: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(again.model.id, parsed.model.id);
        assert_eq!(again.workspace.current_dir, parsed.workspace.current_dir);
        assert_eq!(again.context_window.used_percentage, parsed.context_window.used_percentage);
    }

    #[test]
    fn model_bare_string() {
        let s: SessionInfo = serde_json::from_str(r#"{"model": "claude-sonnet-4-6"}"#).unwrap();
        assert_eq!(s.model.id, "claude-sonnet-4-6");
        assert_eq!(s.model.display_name, "");
    }

    #[test]
    fn model_object() {
        let s: SessionInfo = serde_json::from_str(
            r#"{"model": {"id": "claude-sonnet-4-6", "display_name": "Sonnet 4.6"}}"#,
        )
        .unwrap();
        assert_eq!(s.model.id, "claude-sonnet-4-6");
        assert_eq!(s.model.display_name, "Sonnet 4.6");
    }

    #[test]
    fn model_null_yields_default() {
        let s: SessionInfo = serde_json::from_str(r#"{"model": null}"#).unwrap();
        assert_eq!(s.model.id, "");
    }

    #[test]
    fn null_nested_fields_do_not_abort_parse() {
        // Claude Code's first statusline call on a fresh session can send
        // `null` for nested objects. Without null tolerance, a single null
        // would drop the whole struct to defaults, masking the model name.
        let raw = r#"{
            "model": {"id": "claude-opus-4-7", "display_name": "Opus 4.7"},
            "rate_limits": null,
            "workspace": null,
            "cost": null,
            "context_window": null,
            "effort": null,
            "thinking": null,
            "output_style": null,
            "exceeds_200k_tokens": null,
            "fast_mode": null
        }"#;
        let s: SessionInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(s.model_name(), "Opus 4.7");
        assert_eq!(s.rate_limits.five_hour.used_percentage, 0.0);
        assert_eq!(s.output_style.name, "default");
    }

    #[test]
    fn partial_object_preserves_fields() {
        let s: SessionInfo = parse(r#"{"model": "x", "workspace": {"current_dir": "/c"}}"#);
        assert_eq!(s.model.id, "x");
        assert_eq!(s.workspace.current_dir, "/c");
        assert_eq!(s.cost.total_cost_usd, 0.0);
    }

    #[test]
    fn model_name_prefers_display_name() {
        let s = SessionInfo {
            model: Model { id: "x".into(), display_name: "Sonnet 4.6".into() },
            ..Default::default()
        };
        assert_eq!(s.model_name(), "Sonnet 4.6");
    }

    #[test]
    fn model_name_falls_back_to_id() {
        let s = SessionInfo {
            model: Model { id: "claude-sonnet-4-6".into(), display_name: String::new() },
            ..Default::default()
        };
        assert_eq!(s.model_name(), "claude-sonnet-4-6");
    }

    #[test]
    fn model_name_returns_unknown_when_empty() {
        let s = SessionInfo::default();
        assert_eq!(s.model_name(), "unknown");
    }

    #[test]
    fn model_name_strips_1m_context() {
        let s = SessionInfo {
            model: Model {
                id: String::new(),
                display_name: "Opus 4.7 (1M context)".into(),
            },
            ..Default::default()
        };
        assert_eq!(s.model_name(), "Opus 4.7 1M");
    }

    #[test]
    fn thinking_marker_combos() {
        let mut s = SessionInfo::default();
        s.thinking.enabled = true;
        s.effort.level = "high".into();
        assert_eq!(s.model_thinking(), "high");
        s.fast_mode = true;
        assert_eq!(s.model_thinking(), "high/fast");
        s.thinking.enabled = false;
        s.effort.level = String::new();
        assert_eq!(s.model_thinking(), "fast");
        s.fast_mode = false;
        assert_eq!(s.model_thinking(), "");
    }
}
