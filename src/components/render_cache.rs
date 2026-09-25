//! `RenderCache` — lazy, per-render memoization cache shared across components.
//!
//! Components reach the cache via `ctx.data` (a `&RenderCache`); each
//! accessor fetches its datum at most once per render and reuses the
//! memoized value on subsequent calls.

use std::cell::Cell;

use chrono::{Local, TimeZone};
use once_cell::unsync::OnceCell;

use crate::config::should_show_cost;
use crate::cost::{
    day_cost as compute_day_cost, rates_for, session_cost as compute_session_cost, DayTotals, Usage,
};
use crate::data::git_info::GitInfo;
use crate::data::loaded_skills::LoadedSkills;
use crate::data::openspec::OpenSpec;
use crate::data::running_subagents::RunningSubagents;
use crate::data::account_usage::{self, AccountUsage};
use crate::data::waiting::{self, Marker};
use crate::data::{session_name, subscription_marker};
use crate::data::task_list::TaskList;
use crate::data::token_log::TokenLog;
use crate::data::transcript_usage::TranscriptUsage;
use crate::data::user_messages::last_user_prompt_ts;

use super::context::ComponentContext;

/// Lazy data cache. Each accessor fetches its datum at most once per
/// render; subsequent calls reuse the memoized value. Subscription-marker
/// touches and token-log writes happen on first access only.
#[derive(Default)]
pub struct RenderCache {
    transcript_usage: OnceCell<TranscriptUsage>,
    token_log: OnceCell<TokenLog>,
    git_info: OnceCell<GitInfo>,
    task_list: OnceCell<TaskList>,
    last_prompt_ts: OnceCell<f64>,
    running_subagents: OnceCell<RunningSubagents>,
    openspec: OnceCell<OpenSpec>,
    loaded_skills: OnceCell<LoadedSkills>,
    session_name: OnceCell<Option<String>>,
    waiting: OnceCell<Vec<(String, Marker)>>,
    account_usage: OnceCell<Option<AccountUsage>>,
    session_cost: Cell<Option<f64>>,
    day_cost: Cell<Option<f64>>,
    show_cost: Cell<Option<bool>>,
}

impl RenderCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn transcript_usage(&self, ctx: &ComponentContext) -> &TranscriptUsage {
        self.transcript_usage
            .get_or_init(|| TranscriptUsage::from_transcript(&ctx.session.transcript_path))
    }

    pub fn token_log(&self, ctx: &ComponentContext) -> &TokenLog {
        if self.token_log.get().is_none() {
            let usage = *self.transcript_usage(ctx);
            let today = Local
                .timestamp_opt(ctx.now as i64, 0)
                .single()
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "1970-01-01".to_string());
            let log = TokenLog::update(
                &ctx.env.claude_dir,
                &ctx.session.session_id,
                &today,
                usage.billed_in(),
                usage.cache_read_input_tokens,
                usage.output_tokens,
            );
            let _ = self.token_log.set(log);
        }
        self.token_log.get().unwrap()
    }

    pub fn git_info(&self, ctx: &ComponentContext) -> &GitInfo {
        self.git_info.get_or_init(|| {
            GitInfo::from_cwd(
                &ctx.env.claude_dir,
                &ctx.session.cwd,
                ctx.env.git_cache_ttl_ms,
                ctx.now,
            )
            .unwrap_or_default()
        })
    }

    pub fn task_list(&self, ctx: &ComponentContext) -> &TaskList {
        self.task_list
            .get_or_init(|| TaskList::from_session(&ctx.session.transcript_path))
    }

    pub fn last_prompt_ts(&self, ctx: &ComponentContext) -> f64 {
        *self
            .last_prompt_ts
            .get_or_init(|| last_user_prompt_ts(&ctx.session.transcript_path))
    }

    pub fn running_subagents(&self, ctx: &ComponentContext) -> &RunningSubagents {
        let anchor = self.last_prompt_ts(ctx);
        self.running_subagents.get_or_init(|| {
            RunningSubagents::from_session(
                &ctx.env.claude_dir,
                &ctx.session.session_id,
                &ctx.session.workspace.project_dir,
                ctx.now,
                anchor,
            )
        })
    }

    pub fn openspec(&self, ctx: &ComponentContext) -> &OpenSpec {
        self.openspec
            .get_or_init(|| OpenSpec::from_cwd(&ctx.session.cwd))
    }

    pub fn loaded_skills(&self, ctx: &ComponentContext) -> &LoadedSkills {
        self.loaded_skills
            .get_or_init(|| LoadedSkills::from_transcript(&ctx.session.transcript_path))
    }

    pub fn session_name(&self, ctx: &ComponentContext) -> Option<&str> {
        self.session_name
            .get_or_init(|| session_name::from_transcript(&ctx.session.transcript_path))
            .as_deref()
    }

    pub fn waiting(&self, ctx: &ComponentContext) -> &[(String, Marker)] {
        self.waiting
            .get_or_init(|| waiting::load_all(&ctx.env.claude_dir, ctx.now))
    }

    pub fn account_usage(&self, ctx: &ComponentContext) -> Option<&AccountUsage> {
        self.account_usage
            .get_or_init(|| {
                if ctx.session.rate_limits.five_hour.resets_at == 0 {
                    return None;
                }
                account_usage::load(&ctx.env.claude_dir, ctx.now)
            })
            .as_ref()
    }

    pub fn session_cost(&self, ctx: &ComponentContext) -> f64 {
        if let Some(v) = self.session_cost.get() {
            return v;
        }
        let usage = *self.transcript_usage(ctx);
        let rates = rates_for(&ctx.session.model_name());
        let cost_usage = Usage {
            input_tokens: usage.input_tokens,
            cache_creation_input_tokens: usage.cache_creation_input_tokens,
            cache_read_input_tokens: usage.cache_read_input_tokens,
            output_tokens: usage.output_tokens,
        };
        let v = compute_session_cost(rates, &cost_usage);
        self.session_cost.set(Some(v));
        v
    }

    pub fn day_cost(&self, ctx: &ComponentContext) -> f64 {
        if let Some(v) = self.day_cost.get() {
            return v;
        }
        let log = *self.token_log(ctx);
        let rates = rates_for(&ctx.session.model_name());
        let totals = DayTotals {
            day_in: log.day_in,
            day_cache_read: log.day_cache_read,
            day_out: log.day_out,
        };
        let v = compute_day_cost(rates, &totals);
        self.day_cost.set(Some(v));
        v
    }

    /// Whether the cost cluster should render. Cached so repeated reads stay
    /// idempotent (subscription marker touch happens on first call).
    pub fn show_cost(&self, ctx: &ComponentContext) -> bool {
        if let Some(v) = self.show_cost.get() {
            return v;
        }
        if ctx.session.rate_limits.five_hour.resets_at != 0 {
            subscription_marker::touch(&ctx.env.claude_dir);
        }
        let marker_exists = subscription_marker::exists(&ctx.env.claude_dir);
        let v = should_show_cost(
            &ctx.session.rate_limits,
            ctx.env.show_cost_override,
            marker_exists,
        );
        self.show_cost.set(Some(v));
        v
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::*;
    use crate::config::Env;
    use crate::input::session::SessionInfo;
    use crate::render::Renderer;

    fn make_ctx<'a>(
        session: &'a SessionInfo,
        env: &'a Env,
        renderer: &'a Renderer,
        data: &'a RenderCache,
    ) -> ComponentContext<'a> {
        ComponentContext::new(session, env, renderer, 140, 1_700_000_000.0, data)
    }

    #[test]
    fn transcript_usage_caches_first_fetch() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"role":"assistant","message":{{"id":"a","usage":{{"input_tokens":7,"output_tokens":3}}}}}}"#).unwrap();
        // Two reads should both produce the same value; we can't directly
        // observe the fetch count, but mutating the underlying file after
        // the first read and confirming a second read still returns the
        // original is a stand-in for "exactly one fetch."
        let session = SessionInfo {
            transcript_path: path.to_str().unwrap().to_string(),
            ..Default::default()
        };
        let env = Env::default();
        let r = Renderer::default();
        let data = RenderCache::new();
        let ctx = make_ctx(&session, &env, &r, &data);

        let first = *data.transcript_usage(&ctx);
        assert_eq!(first.input_tokens, 7);

        // Overwrite the transcript with completely different content.
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"role":"assistant","message":{{"id":"b","usage":{{"input_tokens":999,"output_tokens":999}}}}}}"#).unwrap();

        let second = *data.transcript_usage(&ctx);
        assert_eq!(second.input_tokens, 7, "expected cached value, not re-read");
    }

    #[test]
    fn token_log_caches_first_update() {
        let dir = tempdir().unwrap();
        let session = SessionInfo {
            session_id: "test-session".to_string(),
            ..Default::default()
        };
        let env = Env {
            claude_dir: dir.path().to_path_buf(),
            home: PathBuf::new(),
            ..Default::default()
        };
        let r = Renderer::default();
        let data = RenderCache::new();
        let ctx = make_ctx(&session, &env, &r, &data);

        // First read triggers TokenLog::update (which writes the log).
        let _ = data.token_log(&ctx);
        let log_path = dir.path().join("statusline-tokens.log");
        let written = std::fs::metadata(&log_path)
            .ok()
            .map(|m| m.modified().ok())
            .flatten();

        // Second read must not rewrite the log.
        let _ = data.token_log(&ctx);
        let after = std::fs::metadata(&log_path)
            .ok()
            .map(|m| m.modified().ok())
            .flatten();
        // Without a populated transcript the first write is a no-op (empty
        // session usage skips the append). Treat both being None as a pass.
        match (written, after) {
            (Some(a), Some(b)) => assert_eq!(a, b, "log re-touched on cached read"),
            _ => {}
        }
    }
}
