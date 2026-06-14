use super::{AgentSession, SessionReader};
use aftercode_core::session::{CodingEvent, EventType};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::Path;

pub struct CursorReader;

impl SessionReader for CursorReader {
    fn agent_name(&self) -> &'static str {
        "cursor"
    }

    fn latest_session(&self, project_dir: &Path) -> Option<AgentSession> {
        // macOS: ~/Library/Application Support/Cursor/User ; Linux: ~/.config/Cursor/User
        let user_dir = dirs::config_dir()?.join("Cursor").join("User");
        read_session(&user_dir, project_dir)
    }
}

/// Best-effort: never panics. Any error / missing data => None.
/// Public + base-dir injectable for testing.
pub fn read_session(user_dir: &Path, project_dir: &Path) -> Option<AgentSession> {
    let ws_storage = user_dir.join("workspaceStorage");
    let hash_dir = find_workspace(&ws_storage, project_dir)?;

    // composer ids belonging to this workspace
    let ws_db = hash_dir.join("state.vscdb");
    let composers = workspace_composers(&ws_db);
    if composers.is_empty() {
        return None;
    }

    // conversations live in globalStorage cursorDiskKV
    let global_db = user_dir.join("globalStorage").join("state.vscdb");
    let conn = open_ro(&global_db)?;

    let mut events = Vec::new();
    let mut ended_at = 0i64;
    for (cid, updated) in &composers {
        ended_at = ended_at.max(*updated);
        let Some(blob) = kv_get(&conn, &format!("composerData:{cid}")) else {
            continue;
        };
        let Ok(d) = serde_json::from_str::<Value>(&blob) else {
            continue;
        };
        harvest_messages(&d, &mut events);
    }
    if events.is_empty() {
        return None;
    }
    Some(AgentSession {
        agent: "cursor".into(),
        ended_at,
        events,
    })
}

/// Find the workspaceStorage hash dir whose workspace.json folder == project_dir.
fn find_workspace(ws_storage: &Path, project_dir: &Path) -> Option<std::path::PathBuf> {
    let want = canon(project_dir);
    for entry in std::fs::read_dir(ws_storage).ok()?.flatten() {
        let wj = entry.path().join("workspace.json");
        let Ok(txt) = std::fs::read_to_string(&wj) else {
            continue;
        };
        let Ok(d) = serde_json::from_str::<Value>(&txt) else {
            continue;
        };
        if let Some(folder) = d.get("folder").and_then(|v| v.as_str()) {
            let path = folder.strip_prefix("file://").unwrap_or(folder);
            let path = percent_decode(path);
            if canon(Path::new(&path)) == want {
                return Some(entry.path());
            }
        }
    }
    None
}

/// (composerId, lastUpdatedAt_secs) for composers referenced by this workspace.
fn workspace_composers(ws_db: &Path) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    let Some(conn) = open_ro(ws_db) else {
        return out;
    };
    let Some(blob) = kv_get(&conn, "composer.composerData") else {
        return out;
    };
    let Ok(d) = serde_json::from_str::<Value>(&blob) else {
        return out;
    };
    // newer Cursor: allComposers[]; also selected/lastFocused id lists
    if let Some(arr) = d.get("allComposers").and_then(|v| v.as_array()) {
        for c in arr {
            if let Some(id) = c.get("composerId").and_then(|v| v.as_str()) {
                let updated = c
                    .get("lastUpdatedAt")
                    .or_else(|| c.get("createdAt"))
                    .and_then(|v| v.as_i64())
                    .map(|ms| ms / 1000)
                    .unwrap_or(0);
                out.push((id.to_string(), updated));
            }
        }
    }
    for key in ["selectedComposerIds", "lastFocusedComposerIds"] {
        if let Some(arr) = d.get(key).and_then(|v| v.as_array()) {
            for id in arr.iter().filter_map(|v| v.as_str()) {
                if !out.iter().any(|(e, _)| e == id) {
                    out.push((id.to_string(), 0));
                }
            }
        }
    }
    out
}

/// Recursively collect (role, text) from a composer blob. Cursor bubbles use
/// `type` 1=user / 2=assistant (and/or a `role` field) with a `text` field.
fn harvest_messages(d: &Value, events: &mut Vec<CodingEvent>) {
    fn walk(v: &Value, events: &mut Vec<CodingEvent>) {
        match v {
            Value::Object(map) => {
                let text = map.get("text").and_then(|t| t.as_str());
                if let Some(t) = text {
                    if !t.trim().is_empty() {
                        let is_user = match map.get("type").and_then(|x| x.as_i64()) {
                            Some(1) => true,
                            Some(2) => false,
                            _ => map.get("role").and_then(|r| r.as_str()) == Some("user"),
                        };
                        let et = if is_user {
                            EventType::UserPrompt
                        } else {
                            EventType::AgentResponse
                        };
                        events.push(CodingEvent {
                            event_type: et,
                            timestamp: String::new(),
                            content: t.to_string(),
                            metadata: Value::Null,
                        });
                    }
                }
                for (_, child) in map {
                    walk(child, events);
                }
            }
            Value::Array(a) => {
                for child in a {
                    walk(child, events);
                }
            }
            _ => {}
        }
    }
    walk(d, events);
}

fn open_ro(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

fn kv_get(conn: &Connection, key: &str) -> Option<String> {
    // try cursorDiskKV then ItemTable; value may be TEXT or BLOB.
    for table in ["cursorDiskKV", "ItemTable"] {
        let sql = format!("SELECT value FROM {table} WHERE key = ?1");
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let got: rusqlite::Result<String> = stmt.query_row([key], |r| {
                // BLOB or TEXT -> String
                r.get::<_, String>(0).or_else(|_| {
                    r.get::<_, Vec<u8>>(0)
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                })
            });
            if let Ok(v) = got {
                return Some(v);
            }
        }
    }
    None
}

fn canon(p: &Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn percent_decode(s: &str) -> String {
    // minimal: handle %20 etc. without a dep
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn make_db(path: &Path, table: &str, rows: &[(&str, &str)]) {
        let conn = Connection::open(path).unwrap();
        conn.execute(
            &format!("CREATE TABLE {table} (key TEXT PRIMARY KEY, value TEXT)"),
            [],
        )
        .unwrap();
        for (k, v) in rows {
            conn.execute(
                &format!("INSERT INTO {table} (key,value) VALUES (?1,?2)"),
                [k, v],
            )
            .unwrap();
        }
    }

    #[test]
    fn reads_synthetic_cursor_session() {
        let tmp = tempfile::tempdir().unwrap();
        let user = tmp.path().join("User");
        let proj = tmp.path().join("myproj");
        std::fs::create_dir_all(&proj).unwrap();
        let proj_canon = canon(&proj);

        // workspaceStorage/<hash>/{workspace.json,state.vscdb}
        let ws = user.join("workspaceStorage").join("hash1");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("workspace.json"),
            format!("{{\"folder\":\"file://{proj_canon}\"}}"),
        )
        .unwrap();
        make_db(
            &ws.join("state.vscdb"),
            "ItemTable",
            &[(
                "composer.composerData",
                r#"{"allComposers":[{"composerId":"c1","lastUpdatedAt":1700000000000}]}"#,
            )],
        );

        // globalStorage/state.vscdb cursorDiskKV composerData:c1
        let global = user.join("globalStorage");
        std::fs::create_dir_all(&global).unwrap();
        make_db(
            &global.join("state.vscdb"),
            "cursorDiskKV",
            &[(
                "composerData:c1",
                r#"{"composerId":"c1","conversation":[{"type":1,"text":"add redis caching"},{"type":2,"text":"Added a Redis client and config."}]}"#,
            )],
        );

        let sess = read_session(&user, &proj).expect("should read");
        assert_eq!(sess.agent, "cursor");
        assert_eq!(sess.ended_at, 1_700_000_000);
        let c: Vec<_> = sess
            .events
            .iter()
            .map(|e| (e.event_type, e.content.as_str()))
            .collect();
        assert!(c.contains(&(EventType::UserPrompt, "add redis caching")));
        assert!(c.contains(&(EventType::AgentResponse, "Added a Redis client and config.")));
    }

    #[test]
    fn missing_workspace_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let user = tmp.path().join("User");
        std::fs::create_dir_all(user.join("workspaceStorage")).unwrap();
        assert!(read_session(&user, tmp.path()).is_none());
    }

    #[test]
    fn malformed_blob_degrades_to_none() {
        let tmp = tempfile::tempdir().unwrap();
        let user = tmp.path().join("User");
        let proj = tmp.path().join("p");
        std::fs::create_dir_all(&proj).unwrap();
        let pc = canon(&proj);
        let ws = user.join("workspaceStorage").join("h");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("workspace.json"),
            format!("{{\"folder\":\"file://{pc}\"}}"),
        )
        .unwrap();
        make_db(
            &ws.join("state.vscdb"),
            "ItemTable",
            &[("composer.composerData", "{not json")],
        );
        let global = user.join("globalStorage");
        std::fs::create_dir_all(&global).unwrap();
        make_db(&global.join("state.vscdb"), "cursorDiskKV", &[]);
        assert!(read_session(&user, &proj).is_none());
    }
}
