pub mod errors;
pub mod git;
pub mod hooks;

use crate::config::Config;
use crate::privacy::{ignore::Matcher, secrets};
use crate::session;
use aftercode_core::session::{CodingEvent, EventType, Language, SessionContext};
use serde_json::Value;

/// Caps to bound payload size/cost.
const PER_EVENT_CHARS: usize = 8_000;
const TOTAL_CHARS: usize = 150_000;

fn lang_from_str(s: &str) -> Language {
    if s == "he" {
        Language::He
    } else {
        Language::En
    }
}

fn dates_for(from: &str) -> Vec<String> {
    use chrono::{Duration, Utc};
    let today = Utc::now().date_naive();
    let day = if from == "yesterday" {
        today - Duration::days(1)
    } else {
        today
    };
    vec![day.format("%Y-%m-%d").to_string()]
}

/// Which agent session was detected for this repo (for preview/status). None if
/// no agent session found. `forced` corresponds to `--agent`.
pub fn detected_agent(forced: Option<&str>) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    session::detect_best(&cwd, forced).map(|s| s.agent)
}

/// Build a SessionContext from the current directory: the agent session
/// transcript (auto-detected) + the real git diff, with ignore rules applied,
/// secrets redacted, and size capped.
pub fn build(
    cfg: &Config,
    language_override: Option<String>,
    from: &str,
    length: Option<u8>,
    agent: Option<String>,
) -> anyhow::Result<SessionContext> {
    let matcher = Matcher::new(&cfg.privacy.ignore_paths)?;
    let since_days = if from == "yesterday" { 2 } else { 1 };
    let git = git::collect(".", since_days)?;
    let cwd = std::env::current_dir()?;

    let mut events: Vec<CodingEvent> = Vec::new();

    // 1. Agent session transcript (the richest signal).
    if let Some(sess) = session::detect_best(&cwd, agent.as_deref()) {
        events.extend(sess.events);
    }

    // 2. Hook events (back-compat: .aftercode/events/*.jsonl).
    events.extend(hooks::collect(&dates_for(from)).unwrap_or_default());

    // 3. Real git diff hunks as GitDiff events (skip ignored files).
    if cfg.privacy.send_diffs {
        for (path, patch) in &git.diff_hunks {
            if matcher.is_ignored(path) {
                continue;
            }
            events.push(CodingEvent {
                event_type: EventType::GitDiff,
                timestamp: String::new(),
                content: format!("{path}\n{patch}"),
                metadata: Value::Null,
            });
        }
    }

    // Redact secrets from every event, then enforce caps.
    for ev in &mut events {
        ev.content = secrets::redact(&ev.content);
    }
    let events = apply_caps(events);

    let changed_files: Vec<String> = git
        .changed_files
        .into_iter()
        .filter(|f| !matcher.is_ignored(f))
        .collect();
    let diff_summary = git.diff_summary.map(|d| secrets::redact(&d));
    let commit_messages: Vec<String> = git
        .commit_messages
        .iter()
        .map(|m| secrets::redact(m))
        .collect();
    let terminal_errors: Vec<String> = errors::collect()
        .iter()
        .map(|e| secrets::redact(e))
        .collect();

    let language = language_override
        .map(|s| lang_from_str(&s))
        .unwrap_or_else(|| lang_from_str(&cfg.language));
    let minutes = length.unwrap_or(cfg.episode_length_minutes);

    Ok(SessionContext {
        project_id: cfg.project_id.clone(),
        language,
        episode_length_minutes: minutes,
        collected_at: chrono::Utc::now().to_rfc3339(),
        events,
        changed_files,
        git_diff_summary: diff_summary,
        commit_messages,
        terminal_errors,
    })
}

/// Truncate each event to PER_EVENT_CHARS, then keep the most-recent events
/// within TOTAL_CHARS. Prepends a marker if anything was dropped.
fn apply_caps(mut events: Vec<CodingEvent>) -> Vec<CodingEvent> {
    for ev in &mut events {
        if ev.content.chars().count() > PER_EVENT_CHARS {
            let kept: String = ev.content.chars().take(PER_EVENT_CHARS).collect();
            ev.content = format!("{kept}\n[…truncated]");
        }
    }
    let mut total = 0usize;
    let mut kept_rev: Vec<CodingEvent> = Vec::new();
    let original = events.len();
    for ev in events.into_iter().rev() {
        let n = ev.content.chars().count();
        if total + n > TOTAL_CHARS && !kept_rev.is_empty() {
            break;
        }
        total += n;
        kept_rev.push(ev);
    }
    kept_rev.reverse();
    let dropped = original - kept_rev.len();
    if dropped > 0 {
        kept_rev.insert(
            0,
            CodingEvent {
                event_type: EventType::AgentResponse,
                timestamp: String::new(),
                content: format!("[…{dropped} earlier events truncated for size]"),
                metadata: Value::Null,
            },
        );
    }
    kept_rev
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Privacy};
    use std::process::Command;

    fn base_cfg() -> Config {
        Config {
            project_id: "p".into(),
            project_name: "p".into(),
            language: "en".into(),
            episode_length_minutes: 10,
            api_base_url: "http://x".into(),
            privacy: Privacy::default(),
        }
    }

    #[test]
    #[serial_test::serial(fs)]
    fn builds_context_with_diff_and_drops_ignored_and_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        for a in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@e.com"],
            vec!["config", "user.name", "t"],
        ] {
            Command::new("git").args(&a).status().unwrap();
        }
        std::fs::write("keep.rs", "fn x(){}").unwrap();
        std::fs::write(".env", "SECRET=xyz").unwrap();
        Command::new("git")
            .args(["add", "keep.rs"])
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-qm", "add keep"])
            .status()
            .unwrap();
        // change keep.rs (adds a line with a secret) + the ignored .env
        std::fs::write(
            "keep.rs",
            "fn x(){}\nlet api_key = \"abcdef0123456789abcd\";\n",
        )
        .unwrap();
        std::fs::write(".env", "SECRET=changed0123456789").unwrap();

        let ctx = build(&base_cfg(), Some("he".into()), "today", Some(5), None).unwrap();
        std::env::set_current_dir(prev).unwrap();

        assert!(matches!(ctx.language, Language::He));
        assert_eq!(ctx.episode_length_minutes, 5);
        // diff event for keep.rs present
        let diff_ev: Vec<_> = ctx
            .events
            .iter()
            .filter(|e| e.event_type == EventType::GitDiff)
            .collect();
        assert!(diff_ev.iter().any(|e| e.content.starts_with("keep.rs")));
        // .env is ignored -> no diff event for it
        assert!(!diff_ev.iter().any(|e| e.content.starts_with(".env")));
        // secret line in the diff is redacted
        assert!(ctx
            .events
            .iter()
            .all(|e| !e.content.contains("abcdef0123456789")));
    }

    #[test]
    fn caps_truncate_and_mark() {
        let big = "x".repeat(PER_EVENT_CHARS + 500);
        let mut events: Vec<CodingEvent> = (0..40)
            .map(|i| CodingEvent {
                event_type: EventType::AgentResponse,
                timestamp: String::new(),
                content: format!("{i}-{big}"),
                metadata: Value::Null,
            })
            .collect();
        events.push(CodingEvent {
            event_type: EventType::GitDiff,
            timestamp: String::new(),
            content: "newest".into(),
            metadata: Value::Null,
        });
        let out = apply_caps(events);
        // per-event cap applied
        assert!(out
            .iter()
            .all(|e| e.content.chars().count() <= PER_EVENT_CHARS + 32));
        // total cap dropped some -> marker present
        assert!(out[0].content.contains("truncated for size"));
        // newest kept
        assert!(out.iter().any(|e| e.content == "newest"));
    }
}
