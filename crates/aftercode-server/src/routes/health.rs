use axum::response::Html;
use axum::Json;

pub async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Minimal landing page so `aftercode open` shows something instead of a blank
/// page. There is no web UI yet (that's Phase 2) — this explains the API.
pub async fn landing() -> Html<&'static str> {
    Html(
        "<!doctype html><html><head><meta charset=utf-8><title>Aftercode</title>\
         <style>body{font-family:system-ui;max-width:42rem;margin:4rem auto;padding:0 1rem;line-height:1.6}\
         code{background:#f4f1ea;padding:.1rem .3rem;border-radius:3px}</style></head><body>\
         <h1>Aftercode backend</h1>\
         <p>This is the API server. There is no web UI yet (planned for Phase 2).</p>\
         <p>Use the CLI:</p>\
         <pre><code>aftercode preview\naftercode episode</code></pre>\
         <p>Tokens come from the server, not this page:\
         <code>aftercode-server seed-user you@example.com</code>, then \
         <code>aftercode login &lt;token&gt;</code>.</p>\
         <p>Episode audio is served at <code>/static/episodes/&lt;id&gt;.mp3</code>. \
         Health: <a href=\"/healthz\">/healthz</a>.</p>\
         </body></html>",
    )
}

#[cfg(test)]
mod tests {
    use crate::routes::router;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn test_state() -> AppState {
        let cfg = crate::config::Config {
            database_url: std::env::var("TEST_DATABASE_URL")
                .unwrap_or_else(|_| std::env::var("DATABASE_URL").unwrap()),
            bind_addr: "127.0.0.1:0".into(),
            public_url: "http://t".into(),
            llm_provider: "mock".into(),
            anthropic_api_key: None,
            openai_api_key: None,
            elevenlabs_api_key: None,
            host_voice_id: None,
            expert_voice_id: None,
            tts_provider: "mock".into(),
            openai_tts_model: "gpt-4o-mini-tts".into(),
            openai_tts_voice_host: "alloy".into(),
            openai_tts_voice_expert: "onyx".into(),
            blob_store: "mock".into(),
            localfs_dir: "./data".into(),
            s3_bucket: None,
        };
        let db = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&cfg.database_url)
            .await
            .unwrap();
        AppState::for_test(db, cfg)
    }

    #[tokio::test]
    #[serial_test::serial(env)]
    async fn healthz_ok() {
        let app = router(test_state().await);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
