use std::fs;

use crate::git::{GitError, RepoContext};

const STATE_FILE_NAME: &str = "state";

/// Read the previous worktree name from the state file
pub fn read_previous_worktree(ctx: &RepoContext) -> Result<Option<String>, GitError> {
    let state_path = ctx.state_dir().join(STATE_FILE_NAME);

    if !state_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&state_path)
        .map_err(|e| GitError::new(format!("Failed to read state file: {}", e)))?;

    for line in content.lines() {
        if let Some(value) = line.strip_prefix("previous=") {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(Some(value.to_string()));
            }
        }
    }

    Ok(None)
}

/// Save the current worktree name as the previous worktree in the state file
pub fn save_previous_worktree(ctx: &RepoContext, name: &str) -> Result<(), GitError> {
    let state_dir = ctx.state_dir();
    let state_path = state_dir.join(STATE_FILE_NAME);

    // Ensure the state directory exists
    fs::create_dir_all(&state_dir)
        .map_err(|e| GitError::new(format!("Failed to create state directory: {}", e)))?;

    let content = format!("previous={}\n", name);

    fs::write(&state_path, content)
        .map_err(|e| GitError::new(format!("Failed to write state file: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Layout;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Build a bare-layout context whose state dir is `<hub_root>/.wtree`.
    fn bare_ctx(hub_root: PathBuf) -> RepoContext {
        RepoContext {
            layout: Layout::Bare { hub_root },
        }
    }

    fn setup_hub_root() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join(".wtree")).unwrap();
        temp_dir
    }

    #[test]
    fn test_read_previous_worktree_no_state_file() {
        let hub_root = setup_hub_root();
        let ctx = bare_ctx(hub_root.path().to_path_buf());
        let result = read_previous_worktree(&ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_previous_worktree_with_value() {
        let hub_root = setup_hub_root();
        let ctx = bare_ctx(hub_root.path().to_path_buf());
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "previous=main\n").unwrap();

        let result = read_previous_worktree(&ctx).unwrap();
        assert_eq!(result, Some("main".to_string()));
    }

    #[test]
    fn test_read_previous_worktree_empty_value() {
        let hub_root = setup_hub_root();
        let ctx = bare_ctx(hub_root.path().to_path_buf());
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "previous=\n").unwrap();

        let result = read_previous_worktree(&ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_previous_worktree_whitespace_value() {
        let hub_root = setup_hub_root();
        let ctx = bare_ctx(hub_root.path().to_path_buf());
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "previous=   \n").unwrap();

        let result = read_previous_worktree(&ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_previous_worktree_no_previous_line() {
        let hub_root = setup_hub_root();
        let ctx = bare_ctx(hub_root.path().to_path_buf());
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "other=value\n").unwrap();

        let result = read_previous_worktree(&ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_previous_worktree() {
        let hub_root = setup_hub_root();
        let ctx = bare_ctx(hub_root.path().to_path_buf());

        save_previous_worktree(&ctx, "feature").unwrap();

        let state_path = hub_root.path().join(".wtree/state");
        let content = fs::read_to_string(&state_path).unwrap();
        assert_eq!(content, "previous=feature\n");
    }

    #[test]
    fn test_save_previous_worktree_overwrites() {
        let hub_root = setup_hub_root();
        let ctx = bare_ctx(hub_root.path().to_path_buf());
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "previous=old\n").unwrap();

        save_previous_worktree(&ctx, "new").unwrap();

        let content = fs::read_to_string(state_dir.join("state")).unwrap();
        assert_eq!(content, "previous=new\n");
    }

    #[test]
    fn test_save_previous_worktree_creates_state_dir() {
        // No pre-existing state dir: save must create it (lazy state in standard mode).
        let temp_dir = TempDir::new().unwrap();
        let ctx = bare_ctx(temp_dir.path().to_path_buf());

        save_previous_worktree(&ctx, "feature").unwrap();

        let content = fs::read_to_string(temp_dir.path().join(".wtree/state")).unwrap();
        assert_eq!(content, "previous=feature\n");
    }

    #[test]
    fn test_standard_state_dir_location() {
        // In standard mode the state file lands under <common_dir>/wtree/state.
        let temp_dir = TempDir::new().unwrap();
        let common_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&common_dir).unwrap();
        let ctx = RepoContext {
            layout: Layout::Standard {
                main_worktree: temp_dir.path().to_path_buf(),
                common_dir: common_dir.clone(),
            },
        };

        save_previous_worktree(&ctx, "feat").unwrap();

        let content = fs::read_to_string(common_dir.join("wtree/state")).unwrap();
        assert_eq!(content, "previous=feat\n");
    }

    #[test]
    fn test_roundtrip() {
        let hub_root = setup_hub_root();
        let ctx = bare_ctx(hub_root.path().to_path_buf());

        save_previous_worktree(&ctx, "my-worktree").unwrap();
        let result = read_previous_worktree(&ctx).unwrap();

        assert_eq!(result, Some("my-worktree".to_string()));
    }
}
