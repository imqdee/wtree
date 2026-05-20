use std::path::PathBuf;

/// Template content for hooks.toml with commented examples.
pub const HOOKS_TEMPLATE: &str = r#"# wtree hooks configuration
# Define pre/post commands for create, switch, and remove operations.
#
# Pre-hooks run before the command executes (from hub root).
# If a pre-hook fails, the command is aborted.
#
# Post-hooks run after the command completes (from target worktree).
# If a post-hook fails, a warning is logged but the command completes.
#
# Available environment variables in hooks:
#   WT_COMMAND        - Command name (create/switch/remove)
#   WT_WORKTREE_NAME  - Name of the target worktree
#   WT_WORKTREE_PATH  - Absolute path to target worktree
#   WT_HUB_ROOT       - Path to hub root (parent of .bare)
#   WT_BRANCH         - Branch name (create only, if specified)

[create]
# pre = []
# post = ["cp \"$WT_HUB_ROOT/main/.env\" \"$WT_WORKTREE_PATH/\"", "npm install"]

[switch]
# pre = []
# post = []

[remove]
# pre = []
# post = []
"#;

/// Get the path to the global default hooks file (~/.wtree/default-hooks.toml)
fn get_global_default_hooks_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join(".wtree")
            .join("default-hooks.toml")
    })
}

/// Read global default hooks configuration if it exists
pub fn read_global_default_hooks() -> Option<String> {
    let path = get_global_default_hooks_path()?;
    std::fs::read_to_string(&path).ok()
}

/// Resolve the hooks file content to drop into a newly managed repo: the global
/// default from `~/.wtree/default-hooks.toml` if present, otherwise the built-in
/// commented template. Shared by `wt clone` and `wt init`.
pub fn default_hooks_content() -> String {
    read_global_default_hooks().unwrap_or_else(|| HOOKS_TEMPLATE.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // These assertions share one test because they mutate the process-global
    // `HOME` env var; splitting them lets Rust's parallel test runner clobber
    // `HOME` across threads. Keeping them sequential in one function avoids that
    // race without pulling in a serial-test dependency.
    #[test]
    fn test_global_hooks_path_and_fallback() {
        std::env::set_var("HOME", "/home/testuser");
        let path = get_global_default_hooks_path();
        assert_eq!(
            path.unwrap().to_str().unwrap(),
            "/home/testuser/.wtree/default-hooks.toml"
        );

        std::env::set_var("HOME", "/nonexistent/path");
        assert!(read_global_default_hooks().is_none());
        assert!(default_hooks_content().contains("wtree hooks configuration"));
    }
}
