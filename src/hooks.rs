use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Hook phase - determines error handling behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Pre,
    Post,
}

/// Configuration for a single command's hooks
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CommandHooks {
    #[serde(default)]
    pub pre: Vec<String>,
    #[serde(default)]
    pub post: Vec<String>,
}

/// Root configuration loaded from .wtree/hooks.toml
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub create: CommandHooks,
    #[serde(default)]
    pub switch: CommandHooks,
    #[serde(default)]
    pub remove: CommandHooks,
}

/// Context passed to hooks via environment variables
#[derive(Debug, Clone)]
pub struct HookContext {
    pub command: String,
    pub worktree_name: String,
    pub worktree_path: PathBuf,
    pub hub_root: PathBuf,
    pub branch: Option<String>,
}

impl HookContext {
    pub fn new(
        command: &str,
        worktree_name: &str,
        worktree_path: &Path,
        hub_root: &Path,
        branch: Option<&str>,
    ) -> Self {
        Self {
            command: command.to_string(),
            worktree_name: worktree_name.to_string(),
            worktree_path: worktree_path.to_path_buf(),
            hub_root: hub_root.to_path_buf(),
            branch: branch.map(|s| s.to_string()),
        }
    }
}

/// Error type for hook execution
#[derive(Debug)]
pub struct HookError {
    pub message: String,
}

impl std::fmt::Display for HookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for HookError {}

impl HookError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Load hooks configuration from .wtree/hooks.toml
pub fn load_hooks(hub_root: &Path) -> Option<HooksConfig> {
    let hooks_path = hub_root.join(".wtree").join("hooks.toml");
    let content = std::fs::read_to_string(&hooks_path).ok()?;
    toml::from_str(&content).ok()
}

/// Get hooks for a specific command
pub fn get_command_hooks<'a>(config: &'a HooksConfig, command: &str) -> &'a CommandHooks {
    match command {
        "create" => &config.create,
        "switch" => &config.switch,
        "remove" => &config.remove,
        _ => &config.create, // fallback, should never happen
    }
}

/// Run pre-hooks for a command. Returns error if any hook fails.
pub fn run_pre_hooks(
    config: &Option<HooksConfig>,
    context: &HookContext,
) -> Result<(), HookError> {
    let Some(config) = config else {
        return Ok(());
    };

    let hooks = get_command_hooks(config, &context.command);
    run_hooks(&hooks.pre, context, Phase::Pre)
}

/// Run post-hooks for a command. Logs warnings but doesn't return error.
pub fn run_post_hooks(config: &Option<HooksConfig>, context: &HookContext) {
    let Some(config) = config else {
        return;
    };

    let hooks = get_command_hooks(config, &context.command);
    if let Err(e) = run_hooks(&hooks.post, context, Phase::Post) {
        eprintln!("Warning: post-hook failed: {}", e);
    }
}

/// Execute a list of hooks
fn run_hooks(hooks: &[String], context: &HookContext, phase: Phase) -> Result<(), HookError> {
    for hook in hooks {
        run_single_hook(hook, context, phase)?;
    }
    Ok(())
}

/// Execute a single hook command
fn run_single_hook(hook: &str, context: &HookContext, phase: Phase) -> Result<(), HookError> {
    // Determine working directory based on phase
    let working_dir = match phase {
        Phase::Pre => &context.hub_root,
        Phase::Post => {
            // For post-hooks, use worktree path if it exists, otherwise hub root
            if context.worktree_path.exists() {
                &context.worktree_path
            } else {
                &context.hub_root
            }
        }
    };

    let output = Command::new("sh")
        .arg("-c")
        .arg(hook)
        .current_dir(working_dir)
        .env("WT_COMMAND", &context.command)
        .env("WT_WORKTREE_NAME", &context.worktree_name)
        .env("WT_WORKTREE_PATH", context.worktree_path.to_string_lossy().as_ref())
        .env("WT_HUB_ROOT", context.hub_root.to_string_lossy().as_ref())
        .envs(context.branch.as_ref().map(|b| ("WT_BRANCH", b.as_str())))
        .output()
        .map_err(|e| HookError::new(format!("Failed to execute hook '{}': {}", hook, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let phase_name = match phase {
            Phase::Pre => "Pre-hook",
            Phase::Post => "Post-hook",
        };
        return Err(HookError::new(format!(
            "{} '{}' failed: {}",
            phase_name,
            hook,
            stderr.trim()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_parse_empty_config() {
        let config: HooksConfig = toml::from_str("").unwrap();
        assert!(config.create.pre.is_empty());
        assert!(config.create.post.is_empty());
        assert!(config.switch.pre.is_empty());
        assert!(config.switch.post.is_empty());
        assert!(config.remove.pre.is_empty());
        assert!(config.remove.post.is_empty());
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
[create]
pre = ["echo 'Creating...'"]
post = ["npm install", "cp ../.env .env"]

[switch]
pre = []
post = ["npm install"]

[remove]
pre = ["echo 'Cleaning up...'"]
post = []
"#;
        let config: HooksConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(config.create.pre, vec!["echo 'Creating...'"]);
        assert_eq!(config.create.post, vec!["npm install", "cp ../.env .env"]);
        assert!(config.switch.pre.is_empty());
        assert_eq!(config.switch.post, vec!["npm install"]);
        assert_eq!(config.remove.pre, vec!["echo 'Cleaning up...'"]);
        assert!(config.remove.post.is_empty());
    }

    #[test]
    fn test_parse_partial_config() {
        let toml_str = r#"
[create]
post = ["npm install"]
"#;
        let config: HooksConfig = toml::from_str(toml_str).unwrap();

        assert!(config.create.pre.is_empty());
        assert_eq!(config.create.post, vec!["npm install"]);
        assert!(config.switch.pre.is_empty());
        assert!(config.switch.post.is_empty());
    }

    #[test]
    fn test_hook_context_new() {
        let context = HookContext::new(
            "create",
            "feature-branch",
            Path::new("/home/user/project/feature-branch"),
            Path::new("/home/user/project"),
            Some("main"),
        );

        assert_eq!(context.command, "create");
        assert_eq!(context.worktree_name, "feature-branch");
        assert_eq!(
            context.worktree_path,
            PathBuf::from("/home/user/project/feature-branch")
        );
        assert_eq!(context.hub_root, PathBuf::from("/home/user/project"));
        assert_eq!(context.branch, Some("main".to_string()));
    }

    #[test]
    fn test_hook_context_without_branch() {
        let context = HookContext::new(
            "switch",
            "feature-branch",
            Path::new("/home/user/project/feature-branch"),
            Path::new("/home/user/project"),
            None,
        );

        assert_eq!(context.command, "switch");
        assert!(context.branch.is_none());
    }

    #[test]
    fn test_get_command_hooks() {
        let toml_str = r#"
[create]
pre = ["create-pre"]

[switch]
pre = ["switch-pre"]

[remove]
pre = ["remove-pre"]
"#;
        let config: HooksConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(get_command_hooks(&config, "create").pre, vec!["create-pre"]);
        assert_eq!(get_command_hooks(&config, "switch").pre, vec!["switch-pre"]);
        assert_eq!(get_command_hooks(&config, "remove").pre, vec!["remove-pre"]);
    }

    #[test]
    fn test_run_hooks_success() {
        let context = HookContext::new(
            "create",
            "test",
            &env::temp_dir(),
            &env::temp_dir(),
            None,
        );

        let hooks = vec!["true".to_string()];
        let result = run_hooks(&hooks, &context, Phase::Pre);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_hooks_failure() {
        let context = HookContext::new(
            "create",
            "test",
            &env::temp_dir(),
            &env::temp_dir(),
            None,
        );

        let hooks = vec!["false".to_string()];
        let result = run_hooks(&hooks, &context, Phase::Pre);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_hooks_empty() {
        let context = HookContext::new(
            "create",
            "test",
            &env::temp_dir(),
            &env::temp_dir(),
            None,
        );

        let hooks: Vec<String> = vec![];
        let result = run_hooks(&hooks, &context, Phase::Pre);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_pre_hooks_no_config() {
        let context = HookContext::new(
            "create",
            "test",
            &env::temp_dir(),
            &env::temp_dir(),
            None,
        );

        let result = run_pre_hooks(&None, &context);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hook_error_display() {
        let error = HookError::new("test error");
        assert_eq!(format!("{}", error), "test error");
    }

    #[test]
    fn test_load_hooks_missing_file() {
        let result = load_hooks(Path::new("/nonexistent/path"));
        assert!(result.is_none());
    }
}
