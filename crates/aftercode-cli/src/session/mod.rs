//! Reading the developer's coding-agent session (Claude Code, Codex, Cursor)
//! so episodes are built from what actually happened, not just the git diff.

pub mod claude_code;
pub mod codex;
pub mod cursor;

use aftercode_core::session::CodingEvent;
use std::path::Path;

/// A parsed agent session for a project.
pub struct AgentSession {
    pub agent: String,
    pub ended_at: i64, // unix seconds of last activity; higher = more recent
    pub events: Vec<CodingEvent>,
}

/// Reads one coding agent's on-disk session for a project. Total function:
/// returns None on absence or any parse failure — never panics.
pub trait SessionReader {
    fn agent_name(&self) -> &'static str;
    fn latest_session(&self, project_dir: &Path) -> Option<AgentSession>;
}

fn all_readers() -> Vec<Box<dyn SessionReader>> {
    vec![
        Box::new(claude_code::ClaudeCodeReader),
        Box::new(codex::CodexReader),
        Box::new(cursor::CursorReader),
    ]
}

/// Pick the best session for this project. If `forced` is set, only that agent
/// is consulted. Otherwise every reader runs and the most-recently-active wins;
/// ties prefer the richer JSONL transcripts over Cursor's partial SQLite parse.
pub fn detect_best(project_dir: &Path, forced: Option<&str>) -> Option<AgentSession> {
    let mut best: Option<AgentSession> = None;
    for reader in all_readers() {
        if let Some(f) = forced {
            if reader.agent_name() != f {
                continue;
            }
        }
        let Some(sess) = reader.latest_session(project_dir) else {
            continue;
        };
        match &best {
            None => best = Some(sess),
            Some(b) => {
                let newer = sess.ended_at > b.ended_at;
                // within 5s: prefer non-cursor (richer transcript)
                let tie_prefer = (sess.ended_at - b.ended_at).abs() <= 5
                    && b.agent == "cursor"
                    && sess.agent != "cursor";
                if newer || tie_prefer {
                    best = Some(sess);
                }
            }
        }
    }
    best
}

/// Claude Code / Codex encode a project's absolute path into a directory name by
/// replacing every non-alphanumeric character with '-'.
pub fn encode_cwd(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Newest regular file matching `pred` under `dir` (non-recursive), with its mtime secs.
pub(crate) fn newest_file_in<F: Fn(&str) -> bool>(
    dir: &Path,
    pred: F,
) -> Option<(std::path::PathBuf, i64)> {
    let mut best: Option<(std::path::PathBuf, i64)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !pred(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if best.as_ref().map(|(_, m)| mtime > *m).unwrap_or(true) {
            best = Some((entry.path(), mtime));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn encode_cwd_replaces_non_alnum() {
        assert_eq!(
            encode_cwd(&PathBuf::from("/Users/ron/app")),
            "-Users-ron-app"
        );
        assert_eq!(
            encode_cwd(&PathBuf::from("/Users/ron/owla/.claude/wt")),
            "-Users-ron-owla--claude-wt"
        );
    }
}
