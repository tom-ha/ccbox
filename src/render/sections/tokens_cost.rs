use chrono::{Local, TimeZone, Timelike};

use crate::glyphs::{bar, BOLD, ICON_COST, RESET};
use crate::render::format::fmt_tok;
use crate::render::Renderer;
use crate::width::visible_width;

pub const IN_W: usize = 6;
pub const OUT_W: usize = 6;

const GAP: &str = "   ";
const MIN_BAR: usize = 5;
const MAX_BAR: usize = 20;
const MIN_SPAN_SECS: f64 = 5.0 * 60.0;

/// A window always opens at 0%, so the curve starts at `(start, 0)`.
#[derive(Debug, Clone, Default)]
pub struct Trend {
    pub start: f64,
    pub end: f64,
    pub now: f64,
    pub samples: Vec<(f64, f64)>,
    /// How far back the burn rate looks: 30 min for the session, a day for
    /// weekly limits so nights and breaks are part of the rate.
    pub lookback: f64,
}

impl Trend {
    fn points(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        std::iter::once((self.start, 0.0)).chain(
            self.samples
                .iter()
                .copied()
                .filter(|&(ts, _)| ts >= self.start),
        )
    }

    fn pct_at(&self, t: f64) -> f64 {
        let mut prev = (self.start, 0.0);
        for (ts, pct) in self.points() {
            if ts >= t {
                let span = ts - prev.0;
                if span <= 0.0 {
                    return pct;
                }
                return prev.1 + (pct - prev.1) * (t - prev.0) / span;
            }
            prev = (ts, pct);
        }
        prev.1
    }

    /// Least-squares slope over the lookback, so one step in the quantised
    /// usage % doesn't swing the forecast.
    pub fn secs_to_full(&self) -> Option<f64> {
        let t1 = self.now;
        let t0 = (t1 - self.lookback).max(self.start);
        // The window's opening 0% is real data only if it's inside the
        // lookback; interpolating from it otherwise yields the whole-window
        // average, which is what this avoids.
        let anchored = t0 <= self.start || self.samples.iter().any(|&(ts, _)| ts < t0);
        let mut pts: Vec<(f64, f64)> = Vec::new();
        if anchored {
            pts.push((t0, self.pct_at(t0)));
        }
        pts.extend(
            self.samples
                .iter()
                .copied()
                .filter(|&(ts, _)| ts >= t0 && ts <= t1),
        );
        let span = match (pts.first(), pts.last()) {
            (Some(a), Some(b)) => b.0 - a.0,
            _ => 0.0,
        };
        if pts.len() < 2 || span < (self.lookback / 6.0).max(MIN_SPAN_SECS) {
            return None;
        }
        let p1 = self.pct_at(t1);
        if pts.last().is_some_and(|&(ts, _)| ts < t1) {
            pts.push((t1, p1));
        }
        let n = pts.len() as f64;
        let (mt, mp) = pts
            .iter()
            .fold((0.0, 0.0), |(a, b), &(t, p)| (a + t / n, b + p / n));
        let (num, den) = pts.iter().fold((0.0, 0.0), |(num, den), &(t, p)| {
            (num + (t - mt) * (p - mp), den + (t - mt) * (t - mt))
        });
        let slope = num / den;
        (slope > 0.0).then(|| ((100.0 - p1) / slope).max(0.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtraSpend {
    Estimated(f64),
    Actual { used: f64, limit: f64 },
}

#[derive(Debug, Clone)]
pub struct UsageLimit {
    pub label: String,
    pub used_pct: f64,
    pub resets_in_secs: Option<i64>,
    pub now: f64,
    /// Percentage points ahead of (positive) or behind a linear burn of the
    /// window; see [`crate::cost::burndown::burndown_delta`].
    pub pace_delta: Option<f64>,
    /// Feeds the `100% in …` forecast.
    pub trend: Option<Trend>,
    /// How far through the window we are (0–1): where an even burn would
    /// have usage right now. Drawn as the pace marker.
    pub elapsed_frac: Option<f64>,
}

impl UsageLimit {
    fn hits_full_in(&self) -> Option<i64> {
        if self.used_pct >= 100.0 {
            return None;
        }
        let secs = self.trend.as_ref()?.secs_to_full()? as i64;
        (secs < self.resets_in_secs?).then_some(secs)
    }
}

fn fmt_money(v: f64) -> String {
    let abs = v.abs();
    let int_part = abs as u64;
    let int_str = {
        let s = int_part.to_string();
        let bytes = s.as_bytes();
        let mut out = String::with_capacity(s.len() + s.len() / 3);
        for (i, &c) in bytes.iter().enumerate() {
            if i > 0 && (bytes.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c as char);
        }
        out
    };
    let cents = ((abs - int_part as f64) * 100.0).round() as u64;
    let sign = if v < 0.0 { "-" } else { "" };
    format!("{sign}${int_str}.{:02}", cents)
}

pub fn fmt_reset(secs: i64) -> String {
    let s = secs.max(0);
    let (d, h, m) = (s / 86_400, (s % 86_400) / 3600, (s % 3600) / 60);
    if d > 0 {
        format!("{d}d{h}h")
    } else if h > 0 {
        format!("{h}h{m:02}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        "<1m".to_string()
    }
}

/// `9am` / `9:30am` within 24 hours (Claude Code's `/usage` threshold),
/// else `Mon 12pm`. `round_min` snaps to the nearest multiple so forecasts
/// don't pretend to be exact.
pub fn fmt_clock_in<Tz: TimeZone>(tz: &Tz, now: f64, secs: i64, round_min: i64) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let step = round_min.max(1) * 60;
    let at_ts = ((now as i64 + secs.max(0)) + step / 2) / step * step;
    let Some(at) = tz.timestamp_opt(at_ts, 0).single() else {
        return fmt_reset(secs);
    };
    let time = if at.minute() == 0 {
        at.format("%-I%P")
    } else {
        at.format("%-I:%M%P")
    };
    if (at_ts as f64 - now) <= 24.0 * 3600.0 {
        time.to_string()
    } else {
        format!("{} {time}", at.format("%a"))
    }
}

/// Rounds the forecast down so it can never land after the reset it beats.
fn floor_to_quarter_hour(now: f64, secs: i64) -> i64 {
    let at = now as i64 + secs;
    (at - at.rem_euclid(900) - now as i64).max(0)
}

fn fmt_clock(now: f64, secs: i64, round_min: i64) -> String {
    fmt_clock_in(&Local, now, secs, round_min)
}

/// Neighbouring limits that reset together share one `resets` label, shown
/// on the last of them.
fn same_reset_as_next(lims: &[UsageLimit], i: usize) -> bool {
    match (
        lims.get(i).and_then(|l| l.resets_in_secs),
        lims.get(i + 1).and_then(|l| l.resets_in_secs),
    ) {
        (Some(a), Some(b)) => (a - b).abs() < 120,
        _ => false,
    }
}

fn rjust(s: &str, w: usize) -> String {
    format!("{s:>w$}")
}

impl Renderer {
    fn limit_colour(&self, l: &UsageLimit) -> &'static str {
        if l.used_pct >= 90.0 {
            self.theme.alert
        } else if l.used_pct >= 70.0
            || l.pace_delta.is_some_and(|d| d > 10.0)
            || l.hits_full_in().is_some()
        {
            self.theme.warn
        } else {
            self.theme.safe
        }
    }

    fn limit_cluster(&self, l: &UsageLimit, bar_w: usize, show_reset: bool) -> String {
        let t = self.theme;
        let clr = self.limit_colour(l);
        let pct = rjust(&format!("{:.0}%", l.used_pct.clamp(0.0, 100.0)), 4);
        let mut out = format!("{}{}{RESET} {clr}{pct}{RESET}", t.label, l.label);
        if bar_w > 0 {
            let filled = ((l.used_pct / 100.0) * bar_w as f64)
                .round()
                .clamp(0.0, bar_w as f64) as usize;
            let marker = l
                .elapsed_frac
                .map(|f| ((f * bar_w as f64).round() as usize).min(bar_w - 1));
            out.push(' ');
            for i in 0..bar_w {
                if Some(i) == marker {
                    out.push_str(&format!("{BOLD}{}│{RESET}", t.time));
                } else if i < filled && marker.is_some_and(|m| i > m) {
                    out.push_str(&format!("{}▓{RESET}", t.alert));
                } else if i < filled {
                    out.push_str(&format!("{clr}{}{RESET}", bar::FILLED));
                } else {
                    out.push_str(&format!("{}{}{RESET}", t.bar_empty, bar::EMPTY));
                }
            }
        }
        if let Some(secs) = l.hits_full_in() {
            out.push_str(&format!(
                " {}maxed at ~{}{RESET}",
                t.alert,
                fmt_clock(l.now, floor_to_quarter_hour(l.now, secs), 1)
            ));
        }
        if let Some(secs) = l.resets_in_secs.filter(|_| show_reset) {
            let sep = if l.hits_full_in().is_some() {
                " ·"
            } else {
                ""
            };
            out.push_str(&format!(
                "{sep} {}resets {}{}{RESET}",
                t.label,
                t.time,
                fmt_clock(l.now, secs, 1)
            ));
        }
        out
    }

    /// In/out tokens show only when there are no limits (API billing). When
    /// space is tight, later bars drop first, then later limits.
    pub fn tokens_cost(
        &self,
        sess_in: u64,
        sess_out: u64,
        cost: Option<(f64, f64)>,
        limits: &[UsageLimit],
        extra_usage: Option<ExtraSpend>,
        box_width: i32,
    ) -> String {
        let t = self.theme;
        let content_w = (box_width - 3).max(0) as usize;

        let tokens = format!(
            "{}↓{RESET} {}in{RESET} {}{}{RESET}{GAP}{}↑{RESET} {}out{RESET} {}{}{RESET}",
            t.tok_arrow,
            t.label,
            t.tok,
            rjust(&fmt_tok(sess_in), IN_W),
            t.tok_arrow,
            t.label,
            t.tok,
            rjust(&fmt_tok(sess_out), OUT_W),
        );
        let cost_cluster = cost.map(|(sess, day)| {
            format!(
                "{}{ICON_COST}{RESET}{}{}{RESET} {}sess{RESET} {}·{RESET} {}{}{RESET} {}today{RESET}",
                t.safe,
                t.cost,
                fmt_money(sess),
                t.label,
                t.label,
                self.day_cost_colour(day),
                fmt_money(day),
                t.label,
            )
        });

        let extra_cluster = extra_usage.map(|spend| {
            let amount = match spend {
                ExtraSpend::Estimated(usd) => format!("~{}", fmt_money(usd)),
                ExtraSpend::Actual { used, limit } => {
                    format!("{} of {}", fmt_money(used), fmt_money(limit))
                }
            };
            format!(
                "{}{ICON_COST}{RESET} {}extra usage{RESET} {}{amount}{RESET}",
                t.alert, t.label, t.alert,
            )
        });

        let join = |parts: &[String]| parts.join(GAP);
        let mut candidates: Vec<(Vec<String>, &[UsageLimit], bool)> = Vec::new();
        for n in (1..=limits.len()).rev() {
            let lims = &limits[..n];
            let head: Vec<String> = cost_cluster.iter().cloned().collect();
            candidates.push((head.clone(), lims, true));
            candidates.push((head, lims, false));
        }
        let mut head = vec![tokens.clone()];
        head.extend(cost_cluster.clone());
        candidates.push((head, &[], false));

        for (head, lims, with_bars) in &candidates {
            let bare: Vec<String> = head
                .iter()
                .cloned()
                .chain(
                    lims.iter()
                        .enumerate()
                        .map(|(i, l)| self.limit_cluster(l, 0, !same_reset_as_next(lims, i))),
                )
                .chain(extra_cluster.clone())
                .collect();
            let base_w = visible_width(&join(&bare));
            let widths: Vec<usize> = if *with_bars {
                let spare = content_w.saturating_sub(base_w);
                let even = (spare / lims.len()).saturating_sub(1).min(MAX_BAR);
                if even >= MIN_BAR {
                    vec![even; lims.len()]
                } else {
                    let first = spare.saturating_sub(1).min(MAX_BAR);
                    if first < MIN_BAR {
                        continue;
                    }
                    let mut ws = vec![0; lims.len()];
                    ws[0] = first;
                    ws
                }
            } else if base_w > content_w {
                continue;
            } else {
                vec![0; lims.len()]
            };
            let parts: Vec<String> =
                head.iter()
                    .cloned()
                    .chain(
                        lims.iter().zip(&widths).enumerate().map(|(i, (l, &w))| {
                            self.limit_cluster(l, w, !same_reset_as_next(lims, i))
                        }),
                    )
                    .chain(extra_cluster.clone())
                    .collect();
            let line = join(&parts);
            let pad = content_w.saturating_sub(visible_width(&line));
            return format!("{line}{}", " ".repeat(pad));
        }
        " ".repeat(content_w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::strip_ansi;

    fn session(pct: f64) -> UsageLimit {
        UsageLimit {
            label: "session".into(),
            used_pct: pct,
            resets_in_secs: Some(2 * 3600 + 13 * 60),
            now: 0.0,
            pace_delta: None,
            trend: None,
            elapsed_frac: None,
        }
    }

    fn week(pct: f64) -> UsageLimit {
        UsageLimit {
            label: "week".into(),
            used_pct: pct,
            resets_in_secs: Some(3 * 86_400 + 4 * 3600),
            now: 0.0,
            pace_delta: None,
            trend: None,
            elapsed_frac: None,
        }
    }

    #[test]
    fn fmt_money_basics() {
        assert_eq!(fmt_money(0.0), "$0.00");
        assert_eq!(fmt_money(1.0), "$1.00");
        assert_eq!(fmt_money(1.235), "$1.24");
        assert_eq!(fmt_money(1234.5), "$1,234.50");
    }

    #[test]
    fn clock_shows_time_within_a_day_and_weekday_beyond() {
        use chrono::Utc;
        // 2026-09-24 (Thu) 10:00 UTC.
        let now = 1_790_244_000.0;
        let f = |secs, round| fmt_clock_in(&Utc, now, secs, round);
        assert_eq!(f(2 * 3600 + 51 * 60, 1), "12:51pm");
        assert_eq!(f(23 * 3600, 1), "9am", "within 24h, even though tomorrow");
        assert_eq!(f(24 * 3600 + 60, 1), "Fri 10:01am");
        assert_eq!(f(3 * 86_400 + 23 * 3600, 1), "Mon 9am");
        assert_eq!(f(19 * 3600 + 8 * 60, 15), "5:15am");
    }

    #[test]
    fn forecast_rounds_down_so_it_never_passes_the_reset() {
        use chrono::Utc;
        // 2026-09-24 (Thu) 10:00 UTC; forecast 11:38, reset 11:40.
        let now = 1_790_244_000.0;
        let secs = floor_to_quarter_hour(now, 98 * 60);
        assert_eq!(fmt_clock_in(&Utc, now, secs, 1), "11:30am");
        assert_eq!(fmt_clock_in(&Utc, now, 100 * 60, 1), "11:40am");
    }

    #[test]
    fn fmt_reset_tiers() {
        assert_eq!(fmt_reset(30), "<1m");
        assert_eq!(fmt_reset(45 * 60), "45m");
        assert_eq!(fmt_reset(2 * 3600 + 5 * 60), "2h05m");
        assert_eq!(fmt_reset(3 * 86_400 + 4 * 3600 + 59), "3d4h");
        assert_eq!(fmt_reset(-5), "<1m");
    }

    #[test]
    fn tokens_labels_present() {
        let r = Renderer::default();
        let plain = strip_ansi(&r.tokens_cost(120_000, 3_400, None, &[], None, 160)).into_owned();
        assert!(plain.contains("in 120.0K"), "{plain}");
        assert!(plain.contains("out   3.4K"), "{plain}");
    }

    #[test]
    fn cost_cell_renders_when_given() {
        let r = Renderer::default();
        let plain =
            strip_ansi(&r.tokens_cost(1, 2, Some((0.18, 1.42)), &[], None, 160)).into_owned();
        assert!(plain.contains("$0.18 sess · $1.42 today"), "{plain}");
    }

    #[test]
    fn cost_cell_absent_without_cost() {
        let r = Renderer::default();
        let plain =
            strip_ansi(&r.tokens_cost(1, 2, None, &[session(10.0)], None, 160)).into_owned();
        assert!(!plain.contains('$'), "{plain}");
    }

    #[test]
    fn limits_render_pct_and_reset() {
        let r = Renderer::default();
        let line = r.tokens_cost(1, 2, None, &[session(61.0), week(89.0)], None, 140);
        let plain = strip_ansi(&line).into_owned();
        assert!(plain.contains("session  61% "), "{plain}");
        assert_eq!(plain.matches(" resets ").count(), 2, "{plain}");
        assert!(plain.contains("week  89% "), "{plain}");
        assert!(plain.contains(bar::FILLED), "expected bars at 140: {plain}");
        assert!(
            !plain.contains("↓ in"),
            "limits replace the token counts: {plain}"
        );
    }

    #[test]
    fn actual_extra_spend_shows_used_of_limit() {
        let r = Renderer::default();
        let spend = ExtraSpend::Actual {
            used: 130.96,
            limit: 500.0,
        };
        let line = r.tokens_cost(1, 2, None, &[session(100.0)], Some(spend), 140);
        let plain = strip_ansi(&line).into_owned();
        assert!(plain.contains("extra usage $130.96 of $500.00"), "{plain}");
    }

    #[test]
    fn narrow_box_drops_bars_then_tokens_before_limits() {
        let r = Renderer::default();
        let plain = strip_ansi(&r.tokens_cost(1, 2, None, &[session(61.0), week(89.0)], None, 80))
            .into_owned();
        assert!(plain.contains("session"), "{plain}");
        assert!(plain.contains("week"), "{plain}");
        assert!(!plain.contains("↓ in"), "{plain}");
    }

    #[test]
    fn extra_usage_renders_and_survives_narrow_boxes() {
        let r = Renderer::default();
        for box_width in [80, 140] {
            let line = r.tokens_cost(
                1,
                2,
                None,
                &[session(100.0)],
                Some(ExtraSpend::Estimated(3.41)),
                box_width,
            );
            let plain = strip_ansi(&line).into_owned();
            assert!(plain.contains("extra usage ~$3.41"), "{plain}");
            assert_eq!(visible_width(&line) as i32, box_width - 3);
        }
    }

    #[test]
    fn limit_colour_tracks_usage_and_pace() {
        let r = Renderer::default();
        assert_eq!(r.limit_colour(&session(10.0)), r.theme.safe);
        assert_eq!(r.limit_colour(&session(75.0)), r.theme.warn);
        assert_eq!(r.limit_colour(&session(95.0)), r.theme.alert);
        let fast = UsageLimit {
            pace_delta: Some(25.0),
            ..session(30.0)
        };
        assert_eq!(r.limit_colour(&fast), r.theme.warn);
    }

    #[test]
    fn line_width_always_fills_box() {
        let r = Renderer::default();
        let all = [session(61.0), week(89.0)];
        for box_width in [56i32, 70, 80, 100, 130, 160, 200] {
            for lims in [&all[..], &all[..1], &[]] {
                for cost in [None, Some((12.5, 1234.0))] {
                    let line = r.tokens_cost(120_000, 34_000, cost, lims, None, box_width);
                    assert_eq!(
                        visible_width(&line) as i32,
                        box_width - 3,
                        "box={box_width} lims={} cost={cost:?}: {:?}",
                        lims.len(),
                        strip_ansi(&line),
                    );
                }
            }
        }
    }

    const H: f64 = 3600.0;

    /// 5h window, 3h in, burning 20%/h throughout.
    fn steady_trend() -> Trend {
        Trend {
            start: 0.0,
            end: 5.0 * H,
            now: 3.0 * H,
            samples: vec![(1.0 * H, 20.0), (2.0 * H, 40.0), (3.0 * H, 60.0)],
            lookback: 0.5 * H,
        }
    }

    #[test]
    fn forecast_uses_recent_slope() {
        let t = Trend {
            start: 0.0,
            end: 5.0 * H,
            now: 3.0 * H,
            samples: vec![(2.5 * H, 40.0), (3.0 * H, 60.0)],
            lookback: 0.5 * H,
        };
        // Last 30 min: +20% → 40%/h, so the remaining 40% takes 1h.
        let secs = t.secs_to_full().unwrap();
        assert!((secs - H).abs() < 1.0, "{secs}");
    }

    #[test]
    fn forecast_needs_rising_usage() {
        let t = Trend {
            samples: vec![(2.5 * H, 60.0), (3.0 * H, 60.0)],
            ..steady_trend()
        };
        assert_eq!(t.secs_to_full(), None);
    }

    #[test]
    fn bar_fills_to_usage_with_pace_marker_at_elapsed_time() {
        let r = Renderer::default();
        let l = UsageLimit {
            elapsed_frac: Some(0.43),
            ..session(13.0)
        };
        let plain = strip_ansi(&r.limit_cluster(&l, 10, true)).into_owned();
        assert!(plain.contains("13% █░░░│░░░░░ resets"), "{plain}");
    }

    #[test]
    fn marker_sits_inside_fill_when_over_pace() {
        let r = Renderer::default();
        let l = UsageLimit {
            elapsed_frac: Some(0.3),
            ..session(62.0)
        };
        let plain = strip_ansi(&r.limit_cluster(&l, 10, true)).into_owned();
        assert!(plain.contains("███│▓▓░░░░ resets"), "{plain}");
    }

    #[test]
    fn forecast_warns_when_full_before_reset() {
        let r = Renderer::default();
        let on_track = UsageLimit {
            resets_in_secs: Some((2.0 * H) as i64),
            trend: Some(steady_trend()),
            ..session(60.0)
        };
        // 20%/h from 60% → full in 2h, same as the reset.
        let plain = strip_ansi(&r.limit_cluster(&on_track, 10, true)).into_owned();
        assert!(!plain.contains("maxed at"), "{plain}");

        let short = UsageLimit {
            resets_in_secs: Some((3.0 * H) as i64),
            ..on_track
        };
        let plain = strip_ansi(&r.limit_cluster(&short, 10, true)).into_owned();
        assert!(plain.contains(" maxed at ~"), "{plain}");
        assert!(plain.contains(" · resets "), "{plain}");
        assert_eq!(r.limit_colour(&short), r.theme.warn);
    }

    #[test]
    fn pace_line_fills_box_at_all_widths() {
        let r = Renderer::default();
        let lims = [
            UsageLimit {
                elapsed_frac: Some(0.43),
                trend: Some(steady_trend()),
                ..session(13.0)
            },
            UsageLimit {
                elapsed_frac: Some(0.47),
                ..week(81.0)
            },
        ];
        for box_width in [56, 70, 80, 100, 140, 200] {
            let line = r.tokens_cost(1, 2, None, &lims, None, box_width);
            assert_eq!(visible_width(&line) as i32, box_width - 3, "{box_width}");
        }
    }

    #[test]
    fn single_sample_gives_no_forecast() {
        let day = 24.0 * H;
        let t = Trend {
            start: 0.0,
            end: 7.0 * day,
            now: 3.4 * day,
            samples: vec![(3.4 * day, 81.0)],
            lookback: day,
        };
        assert_eq!(t.secs_to_full(), None);
    }

    #[test]
    fn weekly_needs_hours_of_history_not_minutes() {
        let day = 24.0 * H;
        let t = Trend {
            start: 0.0,
            end: 7.0 * day,
            now: 3.4 * day,
            samples: vec![(3.4 * day - 600.0, 80.0), (3.4 * day, 81.0)],
            lookback: day,
        };
        assert_eq!(
            t.secs_to_full(),
            None,
            "10 min of weekly data is too little"
        );
    }

    #[test]
    fn limits_resetting_together_share_one_label() {
        let r = Renderer::default();
        let fable = UsageLimit {
            label: "Fable".into(),
            ..week(82.0)
        };
        let line = r.tokens_cost(1, 2, None, &[session(13.0), week(84.0), fable], None, 140);
        let plain = strip_ansi(&line).into_owned();
        assert_eq!(plain.matches(" resets ").count(), 2, "{plain}");
        let week_at = plain.find("week").unwrap();
        let fable_at = plain.find("Fable").unwrap();
        assert!(!plain[week_at..fable_at].contains("resets"), "{plain}");
    }

    #[test]
    fn one_quantisation_step_does_not_swing_the_forecast() {
        // Flat at 60% for 25 min, then a single 1% step at the last sample.
        let mut samples: Vec<(f64, f64)> = (0..=5)
            .map(|i| (2.5 * H + i as f64 * 300.0, 60.0))
            .collect();
        samples.push((3.0 * H, 61.0));
        let t = Trend {
            start: 0.0,
            end: 5.0 * H,
            now: 3.0 * H,
            samples,
            lookback: 0.5 * H,
        };
        // Two-point would say 2%/h → full in 19.5h; the fit is far gentler.
        let hours = t.secs_to_full().unwrap() / H;
        assert!(hours > 27.0, "{hours}");
    }

    #[test]
    fn weekly_rate_follows_the_last_day_not_the_week() {
        let day = 24.0 * H;
        // Idle at 20% for three days, then 40 points in the last 24h.
        let t = Trend {
            start: 0.0,
            end: 7.0 * day,
            now: 4.0 * day,
            samples: vec![
                (1.0 * day, 20.0),
                (3.0 * day, 20.0),
                (3.5 * day, 40.0),
                (4.0 * day, 60.0),
            ],
            lookback: day,
        };
        // Last day: 40%/day → the remaining 40% lasts ~1 day. The week
        // average (15%/day) would have said ~2.7 days.
        let days = t.secs_to_full().unwrap() / day;
        assert!((days - 1.0).abs() < 0.05, "{days}");
    }
}
