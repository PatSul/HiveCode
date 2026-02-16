use anyhow::Result;
use regex::Regex;
use std::net::IpAddr;
use std::path::Path;
use std::sync::LazyLock;

static SQL_INJECTION_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)('\s*(OR|AND)\s+')").expect("valid regex"),
        Regex::new(r"(?i)(;\s*(DROP|DELETE|UPDATE|INSERT)\s+)").expect("valid regex"),
        Regex::new(r"(?i)(UNION\s+SELECT)").expect("valid regex"),
    ]
});

/// Security gateway that validates commands, URLs, file paths, and content.
/// Ported from the Electron SecurityGateway.
pub struct SecurityGateway {
    dangerous_commands: Vec<Regex>,
    risky_patterns: Vec<Regex>,
    allowed_domains: Vec<String>,
    blocked_path_prefixes: Vec<String>,
}

impl SecurityGateway {
    pub fn new() -> Self {
        Self {
            dangerous_commands: vec![
                Regex::new(r"(?i)\brm\s+-rf\s+/").expect("valid regex"),
                Regex::new(r"(?i)\bmkfs\b").expect("valid regex"),
                Regex::new(r"(?i)\bdd\s+if=").expect("valid regex"),
                Regex::new(r"(?i)>\s*/dev/sd[a-z]").expect("valid regex"),
                Regex::new(r"(?i)\bformat\s+[a-z]:").expect("valid regex"),
                Regex::new(r"(?i)\b(shutdown|reboot|halt|poweroff)\b").expect("valid regex"),
                Regex::new(r"(?i):\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:").expect("valid regex"), // fork bomb
                Regex::new(r"(?i)\bchmod\s+-R\s+777\s+/").expect("valid regex"),
                Regex::new(r"(?i)\bchown\s+-R\s+.*\s+/\s*$").expect("valid regex"),
                Regex::new(r"(?i)\bcurl\b.*\|\s*(ba)?sh").expect("valid regex"),
                Regex::new(r"(?i)\bwget\b.*\|\s*(ba)?sh").expect("valid regex"),
                Regex::new(r"(?i)\bdel\s+/s\s+/q\s+[a-z]:\\").expect("valid regex"),
                Regex::new(r"(?i)\brd\s+/s\s+/q\s+[a-z]:\\").expect("valid regex"),
                Regex::new(r"(?i)\bRemove-Item\s+.*-Recurse\s+-Force\s+[a-z]:\\").expect("valid regex"),
                Regex::new(r"(?i)\bdiskpart\b").expect("valid regex"),
            ],
            risky_patterns: vec![
                Regex::new(r"(?i);\s*(rm|del|format|mkfs)").expect("valid regex"),
                Regex::new(r"(?i)\$\(.*\)").expect("valid regex"),
                Regex::new(r"(?i)`[^`]+`").expect("valid regex"),
                Regex::new(r"(?i)\beval\b").expect("valid regex"),
            ],
            allowed_domains: vec![
                "github.com".into(),
                "raw.githubusercontent.com".into(),
                "api.github.com".into(),
                "registry.npmjs.org".into(),
                "crates.io".into(),
            ],
            blocked_path_prefixes: vec![
                ".ssh".into(),
                ".aws".into(),
                ".gnupg".into(),
                ".config/gcloud".into(),
                ".config\\gcloud".into(),
                "/etc/shadow".into(),
                "/etc/passwd".into(),
            ],
        }
    }

    /// Check if a shell command is safe to execute.
    pub fn check_command(&self, command: &str) -> Result<(), String> {
        for pattern in &self.dangerous_commands {
            if pattern.is_match(command) {
                return Err(format!("Blocked dangerous command: {command}"));
            }
        }
        for pattern in &self.risky_patterns {
            if pattern.is_match(command) {
                return Err(format!("Blocked risky pattern in command: {command}"));
            }
        }
        Ok(())
    }

    /// Validate a URL for fetching.
    pub fn check_url(&self, url: &str) -> Result<(), String> {
        // Must be HTTPS
        if !url.starts_with("https://") {
            return Err("Only HTTPS URLs are allowed".into());
        }

        // Parse host
        let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
        let host = parsed.host_str().ok_or("URL has no host")?;

        // Block private IPs
        if self.is_private_host(host) {
            return Err(format!("Blocked private/local host: {host}"));
        }

        // Check domain allowlist
        if !self
            .allowed_domains
            .iter()
            .any(|d| host.ends_with(d.as_str()))
        {
            return Err(format!("Domain not in allowlist: {host}"));
        }

        Ok(())
    }

    /// Validate a file path for access.
    pub fn check_path(&self, path: &Path) -> Result<(), String> {
        let path_str = path.to_string_lossy();

        // Block system roots (Unix "/" and any Windows drive root like "C:\", "D:/", "E:")
        let is_root = path_str == "/"
            || (path_str.len() <= 3
                && path_str
                    .as_bytes()
                    .first()
                    .is_some_and(|b| b.is_ascii_alphabetic())
                && path_str.as_bytes().get(1) == Some(&b':'));
        if is_root {
            return Err("Access to system root is blocked".into());
        }

        // Block sensitive directories
        for prefix in &self.blocked_path_prefixes {
            if path_str.contains(prefix) {
                return Err(format!("Access to sensitive path blocked: {prefix}"));
            }
        }

        // Resolve to catch traversal — reject if path can't be resolved
        let resolved = path
            .canonicalize()
            .map_err(|_| format!("Cannot resolve path: {path_str}"))?;
        let resolved_str = resolved.to_string_lossy();
        for prefix in &self.blocked_path_prefixes {
            if resolved_str.contains(prefix) {
                return Err(format!(
                    "Path traversal to sensitive directory blocked: {prefix}"
                ));
            }
        }

        Ok(())
    }

    /// Check for common injection patterns in user input.
    pub fn check_injection(&self, input: &str) -> Result<(), String> {
        // SQL injection (patterns compiled once via LazyLock)
        for pat in SQL_INJECTION_PATTERNS.iter() {
            if pat.is_match(input) {
                return Err("Potential SQL injection detected".into());
            }
        }

        // Command injection — flag shell chaining operators in any input
        if input.contains("&&") || input.contains("||") || input.contains(';') {
            return Err("Potential command injection detected".into());
        }

        Ok(())
    }

    fn is_private_host(&self, host: &str) -> bool {
        if host == "localhost" || host.ends_with(".local") {
            return true;
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            return match ip {
                IpAddr::V4(v4) => {
                    v4.is_loopback()
                        || v4.is_private()
                        || v4.is_link_local()
                        || (v4.octets()[0] == 169 && v4.octets()[1] == 254)
                }
                IpAddr::V6(v6) => v6.is_loopback(),
            };
        }
        false
    }
}

impl Default for SecurityGateway {
    fn default() -> Self {
        Self::new()
    }
}
