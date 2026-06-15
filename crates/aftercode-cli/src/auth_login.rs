//! Browser login: `aftercode login` (no token) opens a backend approval page;
//! a loopback server captures the token the backend redirects back with.

use crate::config::Config;
use crate::credentials;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// Resolve backend URL: explicit flag > project config > default.
fn resolve_backend(flag: Option<String>) -> String {
    flag.or_else(|| Config::load().ok().map(|c| c.api_base_url))
        .unwrap_or_else(|| "http://localhost:8080".into())
}

pub fn browser_login(backend_flag: Option<String>) -> anyhow::Result<()> {
    let backend = resolve_backend(backend_flag);
    let state = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let url = format!(
        "{}/cli/authorize?port={}&state={}",
        backend.trim_end_matches('/'),
        port,
        state
    );

    println!("Opening your browser to approve this CLI:\n  {url}\n");
    open_browser(&url);
    println!("Waiting for approval (up to 120s)...");

    let token = wait_for_callback(&listener, &state, Duration::from_secs(120))?;
    credentials::save_token(&token)?;
    println!("Logged in.");
    Ok(())
}

/// Accept one loopback connection, validate state, reply, return the token.
fn wait_for_callback(
    listener: &TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> anyhow::Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return respond_and_extract(stream, expected_state),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    anyhow::bail!(
                        "login timed out. Re-run `aftercode login`, or paste a token: \
                         `aftercode login <token>`."
                    );
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Read the loopback HTTP request, validate state, write a 200 page, return token.
fn respond_and_extract(mut stream: TcpStream, expected_state: &str) -> anyhow::Result<String> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf)?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let line = req.lines().next().unwrap_or("");

    let parsed = parse_callback(line);
    match &parsed {
        Some((_, got_state)) if got_state == expected_state => {
            write_html(
                &mut stream,
                200,
                "Authenticated",
                "✓ You're signed in. Close this tab and return to your terminal.",
            );
        }
        _ => {
            write_html(
                &mut stream,
                400,
                "Login failed",
                "Something went wrong (state mismatch). Return to your terminal and try again.",
            );
        }
    }
    let (token, got_state) = parsed.ok_or_else(|| anyhow::anyhow!("malformed callback request"))?;
    if got_state != expected_state {
        anyhow::bail!("state mismatch on login callback (possible stale tab) — try again");
    }
    Ok(token)
}

/// Parse `GET /callback?token=..&state=.. HTTP/1.1` → (token, state).
pub fn parse_callback(request_line: &str) -> Option<(String, String)> {
    let path = request_line.split_whitespace().nth(1)?; // "/callback?token=..&state=.."
    let query = path.split_once('?')?.1;
    let mut token = None;
    let mut state = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("token", v)) => token = Some(v.to_string()),
            Some(("state", v)) => state = Some(v.to_string()),
            _ => {}
        }
    }
    Some((token?, state?))
}

fn write_html(stream: &mut TcpStream, code: u16, title: &str, msg: &str) {
    let reason = if code == 200 { "OK" } else { "Bad Request" };
    let body = format!(
        "<!doctype html><html><head><meta charset=utf-8><title>{title}</title>\
         <style>body{{font-family:system-ui;max-width:30rem;margin:6rem auto;text-align:center}}</style>\
         </head><body><h2>{msg}</h2></body></html>"
    );
    let resp = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(url).status();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn parse_callback_extracts_token_and_state() {
        let line = "GET /callback?token=ak_abc123&state=S1 HTTP/1.1";
        assert_eq!(
            parse_callback(line),
            Some(("ak_abc123".into(), "S1".into()))
        );
    }

    #[test]
    fn parse_callback_none_without_query() {
        assert_eq!(parse_callback("GET /callback HTTP/1.1"), None);
    }

    #[test]
    fn loopback_captures_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let h =
            thread::spawn(move || wait_for_callback(&listener, "STATE1", Duration::from_secs(5)));
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(b"GET /callback?token=ak_xyz&state=STATE1 HTTP/1.1\r\n\r\n")
            .unwrap();
        // Read the response so the server-side write completes before close.
        let mut resp = String::new();
        let _ = c.read_to_string(&mut resp);
        // The contract is the returned token; the HTTP response is cosmetic and
        // its delivery races with socket close, so we don't assert on it.
        let token = h.join().unwrap().unwrap();
        assert_eq!(token, "ak_xyz");
    }

    #[test]
    fn loopback_rejects_state_mismatch() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = thread::spawn(move || wait_for_callback(&listener, "GOOD", Duration::from_secs(5)));
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(b"GET /callback?token=ak_xyz&state=BAD HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        let _ = c.read_to_string(&mut resp);
        // State mismatch must be rejected (the security-relevant contract).
        assert!(h.join().unwrap().is_err());
    }
}
