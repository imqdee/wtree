use std::fs;
use std::path::Path;

use crate::git::GitError;

const STATE_DIR: &str = ".wtree";
const STATE_FILE: &str = ".wtree/state";

/// Read the previous worktree name from the state file
pub fn read_previous_worktree(hub_root: &Path) -> Result<Option<String>, GitError> {
    let state_path = hub_root.join(STATE_FILE);

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
pub fn save_previous_worktree(hub_root: &Path, name: &str) -> Result<(), GitError> {
    let state_dir = hub_root.join(STATE_DIR);
    let state_path = hub_root.join(STATE_FILE);

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
    use std::fs;
    use tempfile::TempDir;

    fn setup_hub_root() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join(".wtree")).unwrap();
        temp_dir
    }

    #[test]
    fn test_read_previous_worktree_no_state_file() {
        let hub_root = setup_hub_root();
        let result = read_previous_worktree(hub_root.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_previous_worktree_with_value() {
        let hub_root = setup_hub_root();
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "previous=main\n").unwrap();

        let result = read_previous_worktree(hub_root.path()).unwrap();
        assert_eq!(result, Some("main".to_string()));
    }

    #[test]
    fn test_read_previous_worktree_empty_value() {
        let hub_root = setup_hub_root();
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "previous=\n").unwrap();

        let result = read_previous_worktree(hub_root.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_previous_worktree_whitespace_value() {
        let hub_root = setup_hub_root();
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "previous=   \n").unwrap();

        let result = read_previous_worktree(hub_root.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_previous_worktree_no_previous_line() {
        let hub_root = setup_hub_root();
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "other=value\n").unwrap();

        let result = read_previous_worktree(hub_root.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_previous_worktree() {
        let hub_root = setup_hub_root();

        save_previous_worktree(hub_root.path(), "feature").unwrap();

        let state_path = hub_root.path().join(".wtree/state");
        let content = fs::read_to_string(&state_path).unwrap();
        assert_eq!(content, "previous=feature\n");
    }

    #[test]
    fn test_save_previous_worktree_overwrites() {
        let hub_root = setup_hub_root();
        let state_dir = hub_root.path().join(".wtree");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("state"), "previous=old\n").unwrap();

        save_previous_worktree(hub_root.path(), "new").unwrap();

        let content = fs::read_to_string(state_dir.join("state")).unwrap();
        assert_eq!(content, "previous=new\n");
    }

    #[test]
    fn test_roundtrip() {
        let hub_root = setup_hub_root();

        save_previous_worktree(hub_root.path(), "my-worktree").unwrap();
        let result = read_previous_worktree(hub_root.path()).unwrap();

        assert_eq!(result, Some("my-worktree".to_string()));
    }
}
