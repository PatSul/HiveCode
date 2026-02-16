//! Minimal localhost HTTP server for catching OAuth redirect callbacks.
//!
//! During an OAuth 2.0 Authorization Code flow the user's browser is redirected to
//! a local URL (e.g. `http://127.0.0.1:8742/callback?code=XXX`) after they approve
//! the authorization request.  [`OAuthCallbackServer`] spins up a one-shot
//! `TcpListener`, waits for that single request, extracts the `code` query
//! parameter, returns a friendly HTML page to the browser, and hands the
//! authorization code back to the caller.
//!
//! No external HTTP-server crate is required -- the raw HTTP request is parsed
//! directly from the TCP stream.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use anyhow::{bail, Context};
use tracing::{debug, info, warn};

// ── Constants ────────────────────────────────────────────────────────

const LISTEN_ADDR: &str = "127.0.0.1:8742";
const TIMEOUT: Duration = Duration::from_secs(60);

/// HTML page returned to the browser after the authorization code is captured.
const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Authorization Successful</title>
  <style>
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      display: flex; align-items: center; justify-content: center;
      height: 100vh; margin: 0;
      background: #0f0f0f; color: #e0e0e0;
    }
    .card {
      text-align: center; padding: 3rem;
      border: 1px solid #2a2a2a; border-radius: 12px;
      background: #1a1a1a;
    }
    h1 { margin: 0 0 0.5rem; color: #22c55e; }
    p  { margin: 0; color: #888; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Authorization successful</h1>
    <p>You can close this tab and return to HIVE.</p>
  </div>
</body>
</html>"#;

/// Minimal HTML page returned when the request is missing the `code` parameter.
const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Authorization Failed</title>
  <style>
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      display: flex; align-items: center; justify-content: center;
      height: 100vh; margin: 0;
      background: #0f0f0f; color: #e0e0e0;
    }
    .card {
      text-align: center; padding: 3rem;
      border: 1px solid #2a2a2a; border-radius: 12px;
      background: #1a1a1a;
    }
    h1 { margin: 0 0 0.5rem; color: #ef4444; }
    p  { margin: 0; color: #888; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Authorization failed</h1>
    <p>No authorization code was received. Please try again.</p>
  </div>
</body>
</html>"#;

// ── Public API ────────────────────────────────────────────────────────

/// One-shot localhost server that captures an OAuth authorization code.
pub struct OAuthCallbackServer;

impl OAuthCallbackServer {
    /// Start listening on [`LISTEN_ADDR`] and block until an OAuth callback
    /// arrives or the 60-second timeout expires.
    ///
    /// The incoming HTTP request is expected to be a `GET` with a `?code=XXX`
    /// query parameter.  On success the authorization code is returned and a
    /// friendly HTML page is sent to the browser.
    pub fn wait_for_callback() -> anyhow::Result<String> {
        let listener =
            TcpListener::bind(LISTEN_ADDR).context("Failed to bind OAuth callback listener")?;

        info!("OAuth callback server listening on http://{LISTEN_ADDR}");

        // Set a timeout so we don't block forever if the user never completes
        // the authorization flow.
        listener
            .set_nonblocking(false)
            .context("Failed to set listener to blocking mode")?;

        // `accept` itself doesn't support a timeout on `TcpListener`, so we
        // set a timeout on the *accepted* stream below.  To enforce the overall
        // 60-second deadline we configure the listener to non-blocking and poll,
        // but a simpler approach is to accept in blocking mode with a read
        // timeout on the resulting stream.
        //
        // However, `TcpListener::accept` *does* block indefinitely.  The
        // portable way to time-bound it is `set_nonblocking(true)` + a manual
        // poll loop, which is what we do here.
        listener
            .set_nonblocking(true)
            .context("Failed to set listener to non-blocking mode")?;

        let deadline = std::time::Instant::now() + TIMEOUT;
        let poll_interval = Duration::from_millis(250);

        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    debug!("Accepted connection from {addr}");
                    return Self::handle_connection(stream);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        bail!("Timed out waiting for OAuth callback after {TIMEOUT:?}");
                    }
                    std::thread::sleep(poll_interval);
                }
                Err(e) => {
                    return Err(e).context("Failed to accept connection on OAuth callback server");
                }
            }
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────

    /// Read the HTTP request from `stream`, extract the authorization code, and
    /// send back the appropriate HTML response.
    fn handle_connection(mut stream: std::net::TcpStream) -> anyhow::Result<String> {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .context("Failed to set read timeout on stream")?;

        let mut buf = [0u8; 4096];
        let n = stream
            .read(&mut buf)
            .context("Failed to read from OAuth callback connection")?;
        let request = String::from_utf8_lossy(&buf[..n]);

        debug!("Received request:\n{request}");

        // Parse the request line -- we only care about `GET /...?code=XXX ...`
        let request_line = request.lines().next().unwrap_or("");

        let code = Self::extract_code(request_line);

        match &code {
            Some(code) => {
                info!("Captured OAuth authorization code ({} chars)", code.len());
                Self::send_response(&mut stream, 200, SUCCESS_HTML)?;
            }
            None => {
                warn!("OAuth callback did not contain a `code` parameter");
                Self::send_response(&mut stream, 400, ERROR_HTML)?;
            }
        }

        code.context("OAuth callback request did not contain a `code` query parameter")
    }

    /// Extract the value of the `code` query parameter from an HTTP request
    /// line such as `GET /callback?code=abc123&state=xyz HTTP/1.1`.
    fn extract_code(request_line: &str) -> Option<String> {
        // Split "GET /path?query HTTP/1.1" into parts.
        let path = request_line.split_whitespace().nth(1)?;

        let query_string = path.split_once('?').map(|(_, q)| q)?;

        for pair in query_string.split('&') {
            if let Some((key, value)) = pair.split_once('=')
                && key == "code" {
                    let decoded = Self::percent_decode(value);
                    if !decoded.is_empty() {
                        return Some(decoded);
                    }
                }
        }

        None
    }

    /// Minimal percent-decoding (handles `%XX` sequences).
    fn percent_decode(input: &str) -> String {
        let mut output = String::with_capacity(input.len());
        let mut chars = input.chars();

        while let Some(c) = chars.next() {
            if c == '%' {
                let hex: String = chars.by_ref().take(2).collect();
                if hex.len() == 2
                    && let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        output.push(byte as char);
                        continue;
                    }
                // If decoding failed, keep the original characters.
                output.push('%');
                output.push_str(&hex);
            } else if c == '+' {
                output.push(' ');
            } else {
                output.push(c);
            }
        }

        output
    }

    /// Write an HTTP response with the given status code and HTML body.
    fn send_response(
        stream: &mut std::net::TcpStream,
        status: u16,
        body: &str,
    ) -> anyhow::Result<()> {
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            _ => "Unknown",
        };

        let response = format!(
            "HTTP/1.1 {status} {reason}\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {body}",
            body.len(),
        );

        stream
            .write_all(response.as_bytes())
            .context("Failed to write HTTP response to OAuth callback stream")?;

        stream
            .flush()
            .context("Failed to flush OAuth callback stream")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_code_from_typical_request() {
        let line = "GET /callback?code=abc123&state=xyz HTTP/1.1";
        assert_eq!(
            OAuthCallbackServer::extract_code(line),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn extract_code_when_code_is_first_param() {
        let line = "GET /callback?code=hello HTTP/1.1";
        assert_eq!(
            OAuthCallbackServer::extract_code(line),
            Some("hello".to_string())
        );
    }

    #[test]
    fn extract_code_with_percent_encoding() {
        let line = "GET /callback?code=a%20b%2Fc HTTP/1.1";
        assert_eq!(
            OAuthCallbackServer::extract_code(line),
            Some("a b/c".to_string())
        );
    }

    #[test]
    fn extract_code_missing_returns_none() {
        let line = "GET /callback?state=xyz HTTP/1.1";
        assert_eq!(OAuthCallbackServer::extract_code(line), None);
    }

    #[test]
    fn extract_code_empty_value_returns_none() {
        let line = "GET /callback?code=&state=xyz HTTP/1.1";
        assert_eq!(OAuthCallbackServer::extract_code(line), None);
    }

    #[test]
    fn extract_code_no_query_string_returns_none() {
        let line = "GET /callback HTTP/1.1";
        assert_eq!(OAuthCallbackServer::extract_code(line), None);
    }
}
