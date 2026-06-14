use super::{encode_cwd, newest_file_in, AgentSession, SessionReader};
use aftercode_core::session::{CodingEvent, EventType};
use serde_json::Value;
use std::path::Path;

pub struct ClaudeCodeReader;

impl SessionReader for ClaudeCodeReader {
    fn agent_name(&self) -> &'static str {
        "claude-code"
    }

    fn latest_session(&self, project_dir: &Path) -> Option<AgentSession> {
        let home = dirs::home_dir()?;
        let dir = home
            .join(".claude")
            .join("projects")
            .join(encode_cwd(project_dir));
        let (file, mtime) = newest_file_in(&dir, |n| n.ends_with(".jsonl"))?;
        let text = std::fs::read_to_string(&file).ok()?;
        let events = parse_transcript(&text);
        if events.is_empty() {
            return None;
        }
        Some(AgentSession {
            agent: "claude-code".into(),
            ended_at: mtime,
            events,
        })
    }
}

/// Parse Claude Code JSONL into CodingEvents. Public for testing.
pub fn parse_transcript(text: &str) -> Vec<CodingEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let Ok(d) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ty = d.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ty != "user" && ty != "assistant" {
            continue;
        }
        let ts = d
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let content = match d.get("message").and_then(|m| m.get("content")) {
            Some(c) => c,
            None => continue,
        };

        if let Some(s) = content.as_str() {
            // plain user prompt (skip injected command/system wrappers)
            if ty == "user" && !is_noise(s) {
                push(&mut events, EventType::UserPrompt, &ts, s);
            }
            continue;
        }
        let Some(blocks) = content.as_array() else {
            continue;
        };
        for b in blocks {
            let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match bt {
                "text" => {
                    if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                        let et = if ty == "user" {
                            EventType::UserPrompt
                        } else {
                            EventType::AgentResponse
                        };
                        if !is_noise(t) {
                            push(&mut events, et, &ts, t);
                        }
                    }
                }
                "tool_use" => {
                    let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let input = b.get("input").cloned().unwrap_or(Value::Null);
                    match name {
                        "Bash" => {
                            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                                push(
                                    &mut events,
                                    EventType::AgentResponse,
                                    &ts,
                                    &format!("$ {cmd}"),
                                );
                            }
                        }
                        "Edit" | "Write" | "NotebookEdit" => {
                            if let Some(p) = input.get("file_path").and_then(|v| v.as_str()) {
                                push(
                                    &mut events,
                                    EventType::FileChanged,
                                    &ts,
                                    &format!("{name} {p}"),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                _ => {} // skip thinking, tool_result, etc.
            }
        }
    }
    events
}

fn is_noise(s: &str) -> bool {
    let t = s.trim_start();
    t.is_empty()
        || t.starts_with("<local-command")
        || t.starts_with("<command-")
        || t.starts_with("<system-reminder")
        || t.starts_with("Caveat:")
}

fn push(events: &mut Vec<CodingEvent>, et: EventType, ts: &str, content: &str) {
    events.push(CodingEvent {
        event_type: et,
        timestamp: ts.to_string(),
        content: content.to_string(),
        metadata: Value::Null,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_prompt_text_bash_and_edit() {
        let jsonl = r#"
{"type":"summary","summary":"x"}
{"type":"user","timestamp":"t1","message":{"role":"user","content":"add redis caching"}}
{"type":"assistant","timestamp":"t2","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"I'll add a Redis client."},{"type":"tool_use","name":"Bash","input":{"command":"cargo add redis"}},{"type":"tool_use","name":"Write","input":{"file_path":"src/cache.rs","content":"..."}}]}}
{"type":"user","timestamp":"t3","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}
"#;
        let ev = parse_transcript(jsonl);
        let kinds: Vec<_> = ev
            .iter()
            .map(|e| (e.event_type, e.content.as_str()))
            .collect();
        assert!(kinds.contains(&(EventType::UserPrompt, "add redis caching")));
        assert!(kinds.contains(&(EventType::AgentResponse, "I'll add a Redis client.")));
        assert!(kinds.contains(&(EventType::AgentResponse, "$ cargo add redis")));
        assert!(kinds.contains(&(EventType::FileChanged, "Write src/cache.rs")));
        // tool_result + thinking dropped
        assert!(!ev.iter().any(|e| e.content == "ok" || e.content == "hmm"));
    }

    #[test]
    fn drops_noise_wrappers() {
        let jsonl =
            "{\"type\":\"user\",\"message\":{\"content\":\"<command-name>/clear</command-name>\"}}";
        assert!(parse_transcript(jsonl).is_empty());
    }
}
