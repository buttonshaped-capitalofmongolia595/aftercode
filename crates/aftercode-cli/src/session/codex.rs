use super::{AgentSession, SessionReader};
use aftercode_core::session::{CodingEvent, EventType};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct CodexReader;

impl SessionReader for CodexReader {
    fn agent_name(&self) -> &'static str {
        "codex"
    }

    fn latest_session(&self, project_dir: &Path) -> Option<AgentSession> {
        let home = dirs::home_dir()?;
        let base = home.join(".codex").join("sessions");
        let target = project_dir.to_string_lossy().to_string();

        // Walk for rollout-*.jsonl, keep the newest whose session_meta.cwd matches.
        let mut candidates: Vec<(PathBuf, i64)> = Vec::new();
        collect_rollouts(&base, &mut candidates);
        candidates.sort_by_key(|(_, m)| -*m);

        for (file, mtime) in candidates {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if session_cwd(&text).as_deref() != Some(target.as_str()) {
                continue;
            }
            let events = parse_transcript(&text);
            if events.is_empty() {
                continue;
            }
            return Some(AgentSession {
                agent: "codex".into(),
                ended_at: mtime,
                events,
            });
        }
        None
    }
}

fn collect_rollouts(dir: &Path, out: &mut Vec<(PathBuf, i64)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rollouts(&path, out);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                out.push((path, mtime));
            }
        }
    }
}

/// The cwd recorded in the session_meta line, if any.
pub fn session_cwd(text: &str) -> Option<String> {
    for line in text.lines() {
        let Ok(d) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if d.get("type").and_then(|v| v.as_str()) == Some("session_meta") {
            return d
                .get("payload")
                .and_then(|p| p.get("cwd"))
                .and_then(|v| v.as_str())
                .map(String::from);
        }
    }
    None
}

/// Parse Codex rollout JSONL into CodingEvents. Public for testing.
pub fn parse_transcript(text: &str) -> Vec<CodingEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let Ok(d) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if d.get("type").and_then(|v| v.as_str()) != Some("response_item") {
            continue;
        }
        let ts = d
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let Some(p) = d.get("payload") else { continue };
        let pt = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match pt {
            "message" => {
                let role = p.get("role").and_then(|v| v.as_str()).unwrap_or("");
                if role != "user" && role != "assistant" {
                    continue; // skip developer/system
                }
                let text = join_text(p.get("content"));
                if text.trim().is_empty() || is_noise(&text) {
                    continue;
                }
                let et = if role == "user" {
                    EventType::UserPrompt
                } else {
                    EventType::AgentResponse
                };
                events.push(ev(et, &ts, &text));
            }
            "function_call" | "local_shell_call" | "custom_tool_call" => {
                let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
                let args = p
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .or_else(|| p.get("command").and_then(|v| v.as_str()))
                    .unwrap_or("");
                let mut s = format!("tool: {name} {args}");
                s.truncate(600);
                events.push(ev(EventType::AgentResponse, &ts, &s));
            }
            _ => {}
        }
    }
    events
}

fn join_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|b| b.get("text").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn is_noise(s: &str) -> bool {
    let t = s.trim_start();
    t.starts_with("<environment_context>") || t.starts_with("<user_instructions>")
}

fn ev(et: EventType, ts: &str, content: &str) -> CodingEvent {
    CodingEvent {
        event_type: et,
        timestamp: ts.to_string(),
        content: content.to_string(),
        metadata: Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
{"type":"session_meta","timestamp":"t0","payload":{"cwd":"/Users/ron/app","id":"x"}}
{"type":"response_item","timestamp":"t1","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"add rabbitmq queue"}]}}
{"type":"response_item","timestamp":"t2","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Adding a RabbitMQ consumer."}]}}
{"type":"response_item","timestamp":"t3","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":\"docker compose up rabbitmq\"}"}}
{"type":"response_item","timestamp":"t4","payload":{"type":"reasoning","summary":"think"}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context><cwd>/x</cwd></environment_context>"}]}}
"#;

    #[test]
    fn reads_cwd() {
        assert_eq!(session_cwd(SAMPLE).as_deref(), Some("/Users/ron/app"));
    }

    #[test]
    fn extracts_messages_and_calls() {
        let ev = parse_transcript(SAMPLE);
        let c: Vec<_> = ev
            .iter()
            .map(|e| (e.event_type, e.content.as_str()))
            .collect();
        assert!(c.contains(&(EventType::UserPrompt, "add rabbitmq queue")));
        assert!(c.contains(&(EventType::AgentResponse, "Adding a RabbitMQ consumer.")));
        assert!(ev
            .iter()
            .any(|e| e.content.contains("docker compose up rabbitmq")));
        // reasoning + environment_context dropped
        assert!(!ev
            .iter()
            .any(|e| e.content == "think" || e.content.contains("environment_context")));
    }
}
