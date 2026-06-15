use crate::auth::hash_token;
use crate::error::ServerError;
use crate::state::AppState;
use axum::extract::{Form, Query, State};
use axum::response::{Html, IntoResponse, Redirect};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct AuthorizeParams {
    pub port: u16,
    pub state: String,
}

/// Page the CLI opens in the browser. Clicking Approve POSTs to /cli/approve.
pub async fn authorize(Query(p): Query<AuthorizeParams>) -> impl IntoResponse {
    // state is CLI-generated (uuid hex) — alphanumeric, safe to embed.
    let state = sanitize(&p.state);
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8><title>Authorize Aftercode CLI</title>\
         <style>body{{font-family:system-ui;max-width:32rem;margin:5rem auto;padding:0 1rem;\
         text-align:center;line-height:1.6}}button{{font-size:1.1rem;padding:.7rem 2rem;\
         background:#111;color:#fff;border:0;border-radius:8px;cursor:pointer}}</style></head>\
         <body><h1>Authorize Aftercode CLI</h1>\
         <p>This will sign the CLI on this machine into the Aftercode backend.</p>\
         <form method=post action=\"/cli/approve\">\
         <input type=hidden name=port value=\"{}\">\
         <input type=hidden name=state value=\"{}\">\
         <button type=submit>Approve</button></form>\
         <p style=\"color:#888;font-size:.85rem;margin-top:2rem\">Local approval — no identity \
         check. Only use on a backend you trust on your own machine.</p></body></html>",
        p.port, state
    );
    Html(html)
}

#[derive(Deserialize)]
pub struct ApproveParams {
    pub port: u16,
    pub state: String,
}

/// Mint a token and bounce it back to the CLI's loopback listener.
pub async fn approve(
    State(st): State<AppState>,
    Form(p): Form<ApproveParams>,
) -> Result<Redirect, ServerError> {
    let token = format!("ak_{}", Uuid::new_v4().simple());
    let hash = hash_token(&token);
    sqlx::query(
        "INSERT INTO users (email, token_hash) VALUES ('cli@local', $1)
         ON CONFLICT (email) DO UPDATE SET token_hash = EXCLUDED.token_hash",
    )
    .bind(&hash)
    .execute(&st.db)
    .await
    .map_err(|e| ServerError::Other(e.into()))?;

    let url = format!(
        "http://127.0.0.1:{}/callback?token={}&state={}",
        p.port,
        token,
        sanitize(&p.state)
    );
    Ok(Redirect::to(&url))
}

/// Keep only URL/identifier-safe chars (defense-in-depth for embedding).
fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(128)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::router;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[test]
    fn sanitize_strips_unsafe() {
        assert_eq!(sanitize("abc123-_"), "abc123-_");
        assert_eq!(sanitize("a\"<b> c"), "abc");
    }

    async fn test_state() -> AppState {
        let cfg = crate::config::Config {
            database_url: std::env::var("DATABASE_URL").unwrap(),
            bind_addr: "127.0.0.1:0".into(),
            public_url: "http://t".into(),
            llm_provider: "mock".into(),
            anthropic_api_key: None,
            openai_api_key: None,
            elevenlabs_api_key: None,
            host_voice_id: None,
            expert_voice_id: None,
            tts_provider: "mock".into(),
            openai_tts_model: "m".into(),
            openai_tts_voice_host: "alloy".into(),
            openai_tts_voice_expert: "onyx".into(),
            blob_store: "mock".into(),
            localfs_dir: "./data".into(),
            s3_bucket: None,
        };
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect(&cfg.database_url)
            .await
            .unwrap();
        AppState::for_test(db, cfg)
    }

    #[tokio::test]
    #[serial_test::serial(env)]
    async fn authorize_page_has_approve() {
        let app = router(test_state().await);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/cli/authorize?port=1234&state=abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains("Approve"));
        assert!(html.contains("value=\"1234\""));
    }

    #[tokio::test]
    #[serial_test::serial(env)]
    async fn approve_mints_token_and_redirects_to_loopback() {
        let state = test_state().await;
        let db = state.db.clone();
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cli/approve")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("port=4567&state=xyz789"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let loc = resp
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(loc.starts_with("http://127.0.0.1:4567/callback?token=ak_"));
        assert!(loc.contains("&state=xyz789"));
        // user row exists
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email='cli@local'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(n >= 1);
    }
}
