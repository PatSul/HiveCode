use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result status for a single doctor health check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckStatus::Pass => write!(f, "PASS"),
            CheckStatus::Warn => write!(f, "WARN"),
            CheckStatus::Fail => write!(f, "FAIL"),
        }
    }
}

/// A single health-check result produced by the doctor command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub description: String,
    pub status: CheckStatus,
    pub message: String,
    pub fix_suggestion: Option<String>,
}

/// A registered CLI command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliCommand {
    pub name: String,
    pub description: String,
    pub usage: String,
    pub aliases: Vec<String>,
    pub args: Vec<CommandArg>,
}

/// A single argument definition for a CLI command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandArg {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub default_value: Option<String>,
}

/// The result of executing a CLI operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliOutput {
    pub success: bool,
    pub output: String,
    pub exit_code: i32,
}

// ---------------------------------------------------------------------------
// DoctorSummary
// ---------------------------------------------------------------------------

/// Summarised pass/warn/fail counts from a doctor run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorSummary {
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// CliService
// ---------------------------------------------------------------------------

/// In-memory CLI service that manages commands and health checks.
///
/// Provides command registration, alias resolution, help-text generation and
/// a `doctor` subsystem that runs built-in health checks against the local
/// environment.
#[derive(Debug, Clone)]
pub struct CliService {
    commands: HashMap<String, CliCommand>,
}

impl CliService {
    // -- Construction --------------------------------------------------------

    /// Create a new `CliService` pre-populated with the built-in commands.
    pub fn new() -> Self {
        let mut service = Self {
            commands: HashMap::new(),
        };

        // Built-in commands.
        service
            .register_command("doctor", "Run system health checks", "hive doctor")
            .expect("built-in registration");
        service.add_alias("doctor", "check").expect("alias");
        service.add_alias("doctor", "health").expect("alias");

        service
            .register_command("config", "View or edit configuration", "hive config [key] [value]")
            .expect("built-in registration");
        service.add_alias("config", "settings").expect("alias");

        service
            .register_command("chat", "Start a new chat session", "hive chat [message]")
            .expect("built-in registration");

        service
            .register_command("version", "Show version information", "hive version")
            .expect("built-in registration");

        service
            .register_command("help", "Show help information", "hive help [command]")
            .expect("built-in registration");
        service.add_alias("help", "?").expect("alias");

        debug!(count = service.commands.len(), "CLI service initialised with built-in commands");

        service
    }

    // -- Command registration ------------------------------------------------

    /// Register a new command. Returns an error if a command with the same
    /// name already exists.
    pub fn register_command(&mut self, name: &str, description: &str, usage: &str) -> Result<()> {
        if self.commands.contains_key(name) {
            bail!("Command already registered: {name}");
        }
        self.commands.insert(
            name.to_string(),
            CliCommand {
                name: name.to_string(),
                description: description.to_string(),
                usage: usage.to_string(),
                aliases: Vec::new(),
                args: Vec::new(),
            },
        );
        debug!(command = name, "registered CLI command");
        Ok(())
    }

    /// Add an alias for an existing command.
    pub fn add_alias(&mut self, command_name: &str, alias: &str) -> Result<()> {
        let cmd = self
            .commands
            .get_mut(command_name)
            .with_context(|| format!("Command not found: {command_name}"))?;
        if cmd.aliases.contains(&alias.to_string()) {
            bail!("Alias already exists for {command_name}: {alias}");
        }
        cmd.aliases.push(alias.to_string());
        debug!(command = command_name, alias, "added alias");
        Ok(())
    }

    /// Add an argument definition to an existing command.
    pub fn add_arg(&mut self, command_name: &str, arg: CommandArg) -> Result<()> {
        let cmd = self
            .commands
            .get_mut(command_name)
            .with_context(|| format!("Command not found: {command_name}"))?;
        cmd.args.push(arg);
        Ok(())
    }

    // -- Lookup --------------------------------------------------------------

    /// Find a command by name or alias. Returns `None` if not found.
    pub fn get_command(&self, name: &str) -> Option<&CliCommand> {
        // Direct name match first.
        if let Some(cmd) = self.commands.get(name) {
            return Some(cmd);
        }
        // Alias search.
        self.commands
            .values()
            .find(|cmd| cmd.aliases.iter().any(|a| a == name))
    }

    /// Return all registered commands sorted by name.
    pub fn list_commands(&self) -> Vec<&CliCommand> {
        let mut cmds: Vec<_> = self.commands.values().collect();
        cmds.sort_by_key(|c| &c.name);
        cmds
    }

    // -- Help generation -----------------------------------------------------

    /// Generate help text for a single command (looked up by name or alias).
    pub fn generate_help(&self, command_name: &str) -> Result<String> {
        let cmd = self
            .get_command(command_name)
            .with_context(|| format!("Command not found: {command_name}"))?;

        let mut help = String::new();
        help.push_str(&format!("{} - {}\n", cmd.name, cmd.description));
        help.push_str(&format!("\nUsage: {}\n", cmd.usage));

        if !cmd.aliases.is_empty() {
            help.push_str(&format!("Aliases: {}\n", cmd.aliases.join(", ")));
        }

        if !cmd.args.is_empty() {
            help.push_str("\nArguments:\n");
            for arg in &cmd.args {
                let req = if arg.required { "required" } else { "optional" };
                let default = match &arg.default_value {
                    Some(v) => format!(" (default: {v})"),
                    None => String::new(),
                };
                help.push_str(&format!(
                    "  {:<16} {} [{}]{}\n",
                    arg.name, arg.description, req, default
                ));
            }
        }

        Ok(help)
    }

    /// Generate help text for **all** registered commands.
    pub fn generate_help_all(&self) -> String {
        let mut help = String::from("Available commands:\n\n");
        for cmd in self.list_commands() {
            let aliases = if cmd.aliases.is_empty() {
                String::new()
            } else {
                format!(" ({})", cmd.aliases.join(", "))
            };
            help.push_str(&format!("  {:<16} {}{}\n", cmd.name, cmd.description, aliases));
        }
        help
    }

    // -- Doctor checks -------------------------------------------------------

    /// Run all built-in doctor health checks and return the results.
    pub fn run_doctor(&self) -> Vec<DoctorCheck> {
        let mut checks = Vec::new();

        checks.push(self.check_config_file());
        checks.push(self.check_data_directory());
        checks.push(self.check_git_available());
        checks.push(self.check_disk_space());
        checks.push(self.check_network());

        debug!(count = checks.len(), "doctor checks completed");
        checks
    }

    /// Produce a summary of pass/warn/fail counts from a set of checks.
    pub fn doctor_summary(checks: &[DoctorCheck]) -> DoctorSummary {
        let pass = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
        let warn = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
        let fail = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
        DoctorSummary {
            pass,
            warn,
            fail,
            total: checks.len(),
        }
    }

    // -- Individual checks (private) -----------------------------------------

    fn hive_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".hive")
    }

    fn check_config_file(&self) -> DoctorCheck {
        let config_path = Self::hive_dir().join("config.json");
        if config_path.exists() {
            DoctorCheck {
                name: "Config File".to_string(),
                description: "Check if ~/.hive/config.json exists".to_string(),
                status: CheckStatus::Pass,
                message: format!("Config file found at {}", config_path.display()),
                fix_suggestion: None,
            }
        } else {
            DoctorCheck {
                name: "Config File".to_string(),
                description: "Check if ~/.hive/config.json exists".to_string(),
                status: CheckStatus::Warn,
                message: format!("Config file not found at {}", config_path.display()),
                fix_suggestion: Some("Run 'hive config init' to create a default configuration".to_string()),
            }
        }
    }

    fn check_data_directory(&self) -> DoctorCheck {
        let data_dir = Self::hive_dir();
        if data_dir.exists() && data_dir.is_dir() {
            DoctorCheck {
                name: "Data Directory".to_string(),
                description: "Check if ~/.hive/ directory exists".to_string(),
                status: CheckStatus::Pass,
                message: format!("Data directory found at {}", data_dir.display()),
                fix_suggestion: None,
            }
        } else {
            DoctorCheck {
                name: "Data Directory".to_string(),
                description: "Check if ~/.hive/ directory exists".to_string(),
                status: CheckStatus::Warn,
                message: format!("Data directory not found at {}", data_dir.display()),
                fix_suggestion: Some("Run 'hive config init' to create the data directory".to_string()),
            }
        }
    }

    fn check_git_available(&self) -> DoctorCheck {
        match std::process::Command::new("git")
            .arg("--version")
            .output()
        {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                DoctorCheck {
                    name: "Git Available".to_string(),
                    description: "Check if git is available on PATH".to_string(),
                    status: CheckStatus::Pass,
                    message: version,
                    fix_suggestion: None,
                }
            }
            Ok(_) => DoctorCheck {
                name: "Git Available".to_string(),
                description: "Check if git is available on PATH".to_string(),
                status: CheckStatus::Fail,
                message: "git command failed".to_string(),
                fix_suggestion: Some("Install git from https://git-scm.com/".to_string()),
            },
            Err(e) => DoctorCheck {
                name: "Git Available".to_string(),
                description: "Check if git is available on PATH".to_string(),
                status: CheckStatus::Fail,
                message: format!("git not found: {e}"),
                fix_suggestion: Some("Install git from https://git-scm.com/".to_string()),
            },
        }
    }

    fn check_disk_space(&self) -> DoctorCheck {
        DoctorCheck {
            name: "Disk Space".to_string(),
            description: "Check available disk space".to_string(),
            status: CheckStatus::Pass,
            message: "Disk space check passed (placeholder)".to_string(),
            fix_suggestion: None,
        }
    }

    fn check_network(&self) -> DoctorCheck {
        DoctorCheck {
            name: "Network".to_string(),
            description: "Basic network connectivity check".to_string(),
            status: CheckStatus::Pass,
            message: "Network connectivity check passed (placeholder)".to_string(),
            fix_suggestion: None,
        }
    }
}

impl Default for CliService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Construction --------------------------------------------------------

    #[test]
    fn new_creates_service_with_built_in_commands() {
        let service = CliService::new();
        let commands = service.list_commands();
        assert!(commands.len() >= 5, "expected at least 5 built-in commands, got {}", commands.len());
    }

    #[test]
    fn built_in_commands_include_doctor() {
        let service = CliService::new();
        let cmd = service.get_command("doctor");
        assert!(cmd.is_some(), "doctor command should exist");
        let cmd = cmd.unwrap();
        assert_eq!(cmd.name, "doctor");
        assert!(cmd.aliases.contains(&"check".to_string()));
        assert!(cmd.aliases.contains(&"health".to_string()));
    }

    #[test]
    fn built_in_commands_include_config() {
        let service = CliService::new();
        let cmd = service.get_command("config");
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert!(cmd.aliases.contains(&"settings".to_string()));
    }

    #[test]
    fn built_in_commands_include_help() {
        let service = CliService::new();
        let cmd = service.get_command("help");
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert!(cmd.aliases.contains(&"?".to_string()));
    }

    // -- Command registration ------------------------------------------------

    #[test]
    fn register_custom_command() {
        let mut service = CliService::new();
        let result = service.register_command("deploy", "Deploy the application", "hive deploy [env]");
        assert!(result.is_ok());
        let cmd = service.get_command("deploy");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().description, "Deploy the application");
    }

    #[test]
    fn register_duplicate_command_fails() {
        let mut service = CliService::new();
        let result = service.register_command("doctor", "Duplicate", "hive doctor");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("already registered"), "got: {msg}");
    }

    // -- Aliases -------------------------------------------------------------

    #[test]
    fn add_alias_and_lookup() {
        let mut service = CliService::new();
        service
            .register_command("test", "Run tests", "hive test")
            .unwrap();
        service.add_alias("test", "t").unwrap();

        let cmd = service.get_command("t");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "test");
    }

    #[test]
    fn add_alias_to_nonexistent_command_fails() {
        let mut service = CliService::new();
        let result = service.add_alias("nonexistent", "x");
        assert!(result.is_err());
    }

    #[test]
    fn add_duplicate_alias_fails() {
        let mut service = CliService::new();
        service
            .register_command("build", "Build the project", "hive build")
            .unwrap();
        service.add_alias("build", "b").unwrap();
        let result = service.add_alias("build", "b");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Alias already exists"), "got: {msg}");
    }

    #[test]
    fn lookup_by_alias_returns_correct_command() {
        let service = CliService::new();
        // "check" is an alias for "doctor"
        let cmd = service.get_command("check");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "doctor");

        // "health" is also an alias for "doctor"
        let cmd = service.get_command("health");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "doctor");

        // "settings" is an alias for "config"
        let cmd = service.get_command("settings");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "config");

        // "?" is an alias for "help"
        let cmd = service.get_command("?");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "help");
    }

    // -- Arguments -----------------------------------------------------------

    #[test]
    fn add_arg_to_command() {
        let mut service = CliService::new();
        service
            .register_command("greet", "Greet someone", "hive greet <name>")
            .unwrap();
        let arg = CommandArg {
            name: "name".to_string(),
            description: "The person to greet".to_string(),
            required: true,
            default_value: None,
        };
        service.add_arg("greet", arg).unwrap();

        let cmd = service.get_command("greet").unwrap();
        assert_eq!(cmd.args.len(), 1);
        assert_eq!(cmd.args[0].name, "name");
        assert!(cmd.args[0].required);
    }

    #[test]
    fn add_arg_to_nonexistent_command_fails() {
        let mut service = CliService::new();
        let arg = CommandArg {
            name: "flag".to_string(),
            description: "A flag".to_string(),
            required: false,
            default_value: Some("false".to_string()),
        };
        let result = service.add_arg("nonexistent", arg);
        assert!(result.is_err());
    }

    // -- Lookup miss ---------------------------------------------------------

    #[test]
    fn get_nonexistent_command_returns_none() {
        let service = CliService::new();
        assert!(service.get_command("nonexistent").is_none());
    }

    // -- List commands -------------------------------------------------------

    #[test]
    fn list_commands_returns_sorted() {
        let service = CliService::new();
        let commands = service.list_commands();
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "commands should be sorted alphabetically");
    }

    // -- Help generation -----------------------------------------------------

    #[test]
    fn generate_help_for_existing_command() {
        let service = CliService::new();
        let help = service.generate_help("doctor").unwrap();
        assert!(help.contains("doctor"), "help should contain command name");
        assert!(help.contains("health checks"), "help should contain description");
        assert!(help.contains("Usage:"), "help should contain usage section");
        assert!(help.contains("Aliases:"), "help should contain aliases section");
        assert!(help.contains("check"), "help should list aliases");
    }

    #[test]
    fn generate_help_for_alias() {
        let service = CliService::new();
        let help = service.generate_help("check");
        assert!(help.is_ok(), "should find command by alias");
        assert!(help.unwrap().contains("doctor"));
    }

    #[test]
    fn generate_help_for_nonexistent_fails() {
        let service = CliService::new();
        let result = service.generate_help("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn generate_help_includes_args() {
        let mut service = CliService::new();
        service
            .register_command("deploy", "Deploy the app", "hive deploy <env>")
            .unwrap();
        service
            .add_arg(
                "deploy",
                CommandArg {
                    name: "env".to_string(),
                    description: "Target environment".to_string(),
                    required: true,
                    default_value: None,
                },
            )
            .unwrap();
        service
            .add_arg(
                "deploy",
                CommandArg {
                    name: "dry-run".to_string(),
                    description: "Simulate deployment".to_string(),
                    required: false,
                    default_value: Some("false".to_string()),
                },
            )
            .unwrap();

        let help = service.generate_help("deploy").unwrap();
        assert!(help.contains("Arguments:"));
        assert!(help.contains("env"));
        assert!(help.contains("required"));
        assert!(help.contains("dry-run"));
        assert!(help.contains("optional"));
        assert!(help.contains("default: false"));
    }

    #[test]
    fn generate_help_all_lists_all_commands() {
        let service = CliService::new();
        let help = service.generate_help_all();
        assert!(help.contains("Available commands:"));
        assert!(help.contains("doctor"));
        assert!(help.contains("config"));
        assert!(help.contains("chat"));
        assert!(help.contains("version"));
        assert!(help.contains("help"));
    }

    // -- Doctor checks -------------------------------------------------------

    #[test]
    fn run_doctor_returns_five_checks() {
        let service = CliService::new();
        let checks = service.run_doctor();
        assert_eq!(checks.len(), 5, "expected 5 built-in doctor checks");
    }

    #[test]
    fn run_doctor_check_names() {
        let service = CliService::new();
        let checks = service.run_doctor();
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Config File"));
        assert!(names.contains(&"Data Directory"));
        assert!(names.contains(&"Git Available"));
        assert!(names.contains(&"Disk Space"));
        assert!(names.contains(&"Network"));
    }

    #[test]
    fn disk_space_always_passes() {
        let service = CliService::new();
        let checks = service.run_doctor();
        let disk = checks.iter().find(|c| c.name == "Disk Space").unwrap();
        assert_eq!(disk.status, CheckStatus::Pass);
    }

    #[test]
    fn network_always_passes() {
        let service = CliService::new();
        let checks = service.run_doctor();
        let net = checks.iter().find(|c| c.name == "Network").unwrap();
        assert_eq!(net.status, CheckStatus::Pass);
    }

    // -- Doctor summary ------------------------------------------------------

    #[test]
    fn doctor_summary_counts_correctly() {
        let checks = vec![
            DoctorCheck {
                name: "A".to_string(),
                description: String::new(),
                status: CheckStatus::Pass,
                message: String::new(),
                fix_suggestion: None,
            },
            DoctorCheck {
                name: "B".to_string(),
                description: String::new(),
                status: CheckStatus::Pass,
                message: String::new(),
                fix_suggestion: None,
            },
            DoctorCheck {
                name: "C".to_string(),
                description: String::new(),
                status: CheckStatus::Warn,
                message: String::new(),
                fix_suggestion: Some("fix it".to_string()),
            },
            DoctorCheck {
                name: "D".to_string(),
                description: String::new(),
                status: CheckStatus::Fail,
                message: String::new(),
                fix_suggestion: Some("install something".to_string()),
            },
        ];
        let summary = CliService::doctor_summary(&checks);
        assert_eq!(summary.pass, 2);
        assert_eq!(summary.warn, 1);
        assert_eq!(summary.fail, 1);
        assert_eq!(summary.total, 4);
    }

    #[test]
    fn doctor_summary_empty_checks() {
        let summary = CliService::doctor_summary(&[]);
        assert_eq!(summary.pass, 0);
        assert_eq!(summary.warn, 0);
        assert_eq!(summary.fail, 0);
        assert_eq!(summary.total, 0);
    }

    // -- Serialization -------------------------------------------------------

    #[test]
    fn check_status_serialization_roundtrip() {
        let statuses = vec![CheckStatus::Pass, CheckStatus::Warn, CheckStatus::Fail];
        for status in &statuses {
            let json = serde_json::to_string(status).expect("serialize");
            let restored: CheckStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(&restored, status);
        }
    }

    #[test]
    fn cli_command_serialization_roundtrip() {
        let cmd = CliCommand {
            name: "test".to_string(),
            description: "A test command".to_string(),
            usage: "hive test".to_string(),
            aliases: vec!["t".to_string()],
            args: vec![CommandArg {
                name: "verbose".to_string(),
                description: "Verbose output".to_string(),
                required: false,
                default_value: Some("false".to_string()),
            }],
        };
        let json = serde_json::to_string(&cmd).expect("serialize");
        let restored: CliCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.name, cmd.name);
        assert_eq!(restored.aliases, cmd.aliases);
        assert_eq!(restored.args.len(), 1);
    }

    #[test]
    fn cli_output_serialization_roundtrip() {
        let output = CliOutput {
            success: true,
            output: "All good".to_string(),
            exit_code: 0,
        };
        let json = serde_json::to_string(&output).expect("serialize");
        let restored: CliOutput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.success, true);
        assert_eq!(restored.output, "All good");
        assert_eq!(restored.exit_code, 0);
    }

    #[test]
    fn doctor_check_serialization_roundtrip() {
        let check = DoctorCheck {
            name: "Test Check".to_string(),
            description: "A test check".to_string(),
            status: CheckStatus::Warn,
            message: "Something is off".to_string(),
            fix_suggestion: Some("Fix it".to_string()),
        };
        let json = serde_json::to_string(&check).expect("serialize");
        let restored: DoctorCheck = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.name, "Test Check");
        assert_eq!(restored.status, CheckStatus::Warn);
        assert!(restored.fix_suggestion.is_some());
    }

    // -- CheckStatus display -------------------------------------------------

    #[test]
    fn check_status_display() {
        assert_eq!(format!("{}", CheckStatus::Pass), "PASS");
        assert_eq!(format!("{}", CheckStatus::Warn), "WARN");
        assert_eq!(format!("{}", CheckStatus::Fail), "FAIL");
    }

    // -- Default trait -------------------------------------------------------

    #[test]
    fn default_creates_same_as_new() {
        let from_new = CliService::new();
        let from_default = CliService::default();
        assert_eq!(from_new.list_commands().len(), from_default.list_commands().len());
    }

    // -- Git check actually runs ---------------------------------------------

    #[test]
    fn git_check_returns_valid_status() {
        let service = CliService::new();
        let checks = service.run_doctor();
        let git = checks.iter().find(|c| c.name == "Git Available").unwrap();
        // Git is either available or not - both are valid outcomes.
        match &git.status {
            CheckStatus::Pass => {
                assert!(git.message.contains("git"), "pass message should mention git");
                assert!(git.fix_suggestion.is_none());
            }
            CheckStatus::Fail => {
                assert!(git.fix_suggestion.is_some());
            }
            CheckStatus::Warn => panic!("git check should not return Warn"),
        }
    }
}
