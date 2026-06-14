use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    He,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    UserPrompt,
    AgentResponse,
    FileChanged,
    TerminalError,
    GitDiff,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingEvent {
    pub event_type: EventType,
    pub timestamp: String,
    pub content: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub project_id: String,
    pub language: Language,
    pub episode_length_minutes: u8,
    pub collected_at: String,
    #[serde(default)]
    pub events: Vec<CodingEvent>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub git_diff_summary: Option<String>,
    #[serde(default)]
    pub commit_messages: Vec<String>,
    #[serde(default)]
    pub terminal_errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Language::He).unwrap(), "\"he\"");
    }

    #[test]
    fn session_roundtrips() {
        let s = SessionContext {
            project_id: "proj_1".into(),
            language: Language::En,
            episode_length_minutes: 10,
            collected_at: "2026-06-14T19:00:00Z".into(),
            events: vec![],
            changed_files: vec!["a.rs".into()],
            git_diff_summary: Some("x".into()),
            commit_messages: vec![],
            terminal_errors: vec![],
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: SessionContext = serde_json::from_str(&j).unwrap();
        assert_eq!(back.project_id, "proj_1");
        assert_eq!(back.episode_length_minutes, 10);
    }
}
