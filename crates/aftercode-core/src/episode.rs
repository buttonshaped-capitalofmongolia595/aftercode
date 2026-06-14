use crate::session::Language;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearningTopic {
    pub title: String,
    pub summary: String,
    pub evidence: Vec<String>,
    pub knowledge_gap: String,
    pub difficulty: String,
    pub priority: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Host,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptSegment {
    pub speaker: Speaker,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Quiz {
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodeScript {
    pub title: String,
    pub language: Language,
    pub segments: Vec<ScriptSegment>,
    pub summary_points: Vec<String>,
    pub quiz: Option<Quiz>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeStatus {
    Queued,
    ExtractingTopics,
    WritingScript,
    GeneratingAudio,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeSummary {
    pub id: String,
    pub title: String,
    pub project_name: String,
    pub language: Language,
    pub status: EpisodeStatus,
    pub duration_seconds: Option<i32>,
    pub topics: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeDetail {
    pub id: String,
    pub title: String,
    pub language: Language,
    pub status: EpisodeStatus,
    pub audio_url: Option<String>,
    pub duration_seconds: Option<i32>,
    pub summary: Option<String>,
    pub transcript_text: Option<String>,
    pub topics: Vec<LearningTopic>,
    pub script: Option<EpisodeScript>,
    pub error: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_snake_case() {
        assert_eq!(
            serde_json::to_string(&EpisodeStatus::ExtractingTopics).unwrap(),
            "\"extracting_topics\""
        );
    }
    #[test]
    fn speaker_lowercase() {
        assert_eq!(
            serde_json::to_string(&Speaker::Expert).unwrap(),
            "\"expert\""
        );
    }
}
