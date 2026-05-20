use serde::Deserialize;

use crate::git::RepoContext;

const CONFIG_FILE_NAME: &str = "config.toml";

/// Optional per-repo configuration loaded from `<state_dir>/config.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WtreeConfig {
    /// Override for where standard-layout worktrees are created. Relative paths
    /// resolve against the main worktree, absolute paths are used as-is.
    #[serde(default)]
    pub worktree_base: Option<String>,
}

/// Load config from `<state_dir>/config.toml`.
///
/// A missing file returns the default. A parse error warns to stderr and falls
/// back to the default rather than aborting the command, so a malformed config
/// never bricks `wt`.
pub fn load_config(ctx: &RepoContext) -> WtreeConfig {
    let path = ctx.state_dir().join(CONFIG_FILE_NAME);

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return WtreeConfig::default(),
    };

    match toml::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "Warning: failed to parse {}: {}. Using defaults.",
                path.display(),
                e
            );
            WtreeConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Layout;
    use std::fs;
    use tempfile::TempDir;

    fn ctx_with_config(contents: Option<&str>) -> (TempDir, RepoContext) {
        let tmp = TempDir::new().unwrap();
        let state_dir = tmp.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        if let Some(c) = contents {
            fs::write(state_dir.join("config.toml"), c).unwrap();
        }
        let ctx = RepoContext {
            layout: Layout::Bare {
                hub_root: tmp.path().to_path_buf(),
            },
        };
        (tmp, ctx)
    }

    #[test]
    fn test_missing_config_is_default() {
        let (_tmp, ctx) = ctx_with_config(None);
        let cfg = load_config(&ctx);
        assert!(cfg.worktree_base.is_none());
    }

    #[test]
    fn test_config_with_worktree_base() {
        let (_tmp, ctx) = ctx_with_config(Some("worktree_base = \"wt\"\n"));
        let cfg = load_config(&ctx);
        assert_eq!(cfg.worktree_base.as_deref(), Some("wt"));
    }

    #[test]
    fn test_config_absolute_worktree_base() {
        let (_tmp, ctx) = ctx_with_config(Some("worktree_base = \"/abs/elsewhere\"\n"));
        let cfg = load_config(&ctx);
        assert_eq!(cfg.worktree_base.as_deref(), Some("/abs/elsewhere"));
    }

    #[test]
    fn test_garbage_config_falls_back_to_default() {
        let (_tmp, ctx) = ctx_with_config(Some("this is not = valid = toml ["));
        let cfg = load_config(&ctx);
        assert!(cfg.worktree_base.is_none());
    }

    #[test]
    fn test_empty_config_is_default() {
        let (_tmp, ctx) = ctx_with_config(Some(""));
        let cfg = load_config(&ctx);
        assert!(cfg.worktree_base.is_none());
    }
}
