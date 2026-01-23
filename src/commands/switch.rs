use std::fs;

use crate::git::{find_hub_root, get_worktree_list, GitError};

/// Check if a filename should be copied as an env file
/// Returns true for files starting with ".env" except ".env.example"
pub fn should_copy_env_file(filename: &str) -> bool {
    filename.starts_with(".env") && filename != ".env.example"
}

pub fn run(name: &str, copy_envs: bool) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;
    let worktrees = get_worktree_list(&hub_root)?;

    // Find the worktree by name (matching the directory name)
    for wt in &worktrees {
        if let Some(dir_name) = wt.path.file_name() {
            if dir_name.to_string_lossy() == name {
                // Copy .env* files if requested
                if copy_envs {
                    let source = std::env::current_dir()?;
                    if let Ok(entries) = fs::read_dir(&source) {
                        for entry in entries.flatten() {
                            let file_name = entry.file_name();
                            let file_name_str = file_name.to_string_lossy();
                            if should_copy_env_file(&file_name_str) && entry.path().is_file() {
                                fs::copy(entry.path(), wt.path.join(&file_name))?;
                            }
                        }
                    }
                }

                // Print path for shell wrapper to cd into
                println!("{}", wt.path.display());
                return Ok(());
            }
        }
    }

    Err(Box::new(GitError::new(format!(
        "Worktree '{}' not found. Use 'wt list' to see available worktrees.",
        name
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_copy_env_file_basic() {
        assert!(should_copy_env_file(".env"));
    }

    #[test]
    fn test_should_copy_env_file_local() {
        assert!(should_copy_env_file(".env.local"));
    }

    #[test]
    fn test_should_copy_env_file_production() {
        assert!(should_copy_env_file(".env.production"));
    }

    #[test]
    fn test_should_copy_env_file_development() {
        assert!(should_copy_env_file(".env.development"));
    }

    #[test]
    fn test_should_copy_env_file_with_suffix() {
        assert!(should_copy_env_file(".env.staging"));
        assert!(should_copy_env_file(".env.test"));
        assert!(should_copy_env_file(".env.local.backup"));
    }

    #[test]
    fn test_should_not_copy_env_example() {
        assert!(!should_copy_env_file(".env.example"));
    }

    #[test]
    fn test_should_not_copy_non_env_files() {
        assert!(!should_copy_env_file("env"));
        assert!(!should_copy_env_file("config.env"));
    }

    #[test]
    fn test_should_copy_env_prefixed_files() {
        // Files starting with ".env" are copied (current behavior)
        assert!(should_copy_env_file(".environment"));
        assert!(should_copy_env_file(".envrc"));
    }

    #[test]
    fn test_should_not_copy_hidden_files() {
        assert!(!should_copy_env_file(".gitignore"));
        assert!(!should_copy_env_file(".dockerignore"));
    }

    #[test]
    fn test_should_not_copy_empty_string() {
        assert!(!should_copy_env_file(""));
    }
}
