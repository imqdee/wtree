use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct GitError {
    pub message: String,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GitError {}

impl GitError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Execute a git command in a specific directory
pub fn run_git_in_dir(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| GitError::new(format!("Failed to execute git: {}", e)))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(GitError::new(stderr))
    }
}

/// Find the hub root (bare repo root) from the current directory.
/// Works whether we're in a worktree or in the hub itself.
pub fn find_hub_root() -> Result<PathBuf, GitError> {
    let current_dir = std::env::current_dir()
        .map_err(|e| GitError::new(format!("Cannot get current dir: {}", e)))?;

    // Walk up the directory tree looking for .bare directory or .git file
    let mut dir = current_dir.as_path();
    loop {
        // Check if this directory contains .bare (hub root)
        let bare_path = dir.join(".bare");
        if bare_path.exists() && bare_path.is_dir() {
            return Ok(dir.to_path_buf());
        }

        // Check if there's a .git file (not directory) pointing to .bare
        let git_path = dir.join(".git");
        if git_path.exists() && git_path.is_file() {
            // Read the .git file to find the bare repo
            let content = std::fs::read_to_string(&git_path)
                .map_err(|e| GitError::new(format!("Cannot read .git file: {}", e)))?;

            if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                let gitdir = gitdir.trim();
                let gitdir_path = if Path::new(gitdir).is_absolute() {
                    PathBuf::from(gitdir)
                } else {
                    dir.join(gitdir)
                };

                // The gitdir should point to .bare, so hub root is its parent
                if let Some(hub_root) = gitdir_path.parent() {
                    let hub_root = hub_root
                        .canonicalize()
                        .unwrap_or_else(|_| hub_root.to_path_buf());
                    if hub_root.join(".bare").exists() {
                        return Ok(hub_root);
                    }
                }
            }
        }

        // Move up one directory
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    Err(GitError::new(
        "Not inside a wtree repository (cannot find .bare directory)",
    ))
}

/// Worktree information
#[derive(Debug)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
}

/// Parse git worktree list --porcelain output into structured data
pub fn parse_worktree_list(output: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            // Save previous worktree if exists
            if let (Some(path), Some(head)) = (current_path.take(), current_head.take()) {
                worktrees.push(Worktree {
                    path,
                    head,
                    branch: current_branch.take(),
                });
            }
            current_path = Some(PathBuf::from(path));
            current_branch = None;
        } else if let Some(head) = line.strip_prefix("HEAD ") {
            current_head = Some(head.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(branch.to_string());
        } else if line == "bare" {
            // Mark bare repo
            current_head = Some("(bare)".to_string());
        }
    }

    // Don't forget the last one
    if let (Some(path), Some(head)) = (current_path, current_head) {
        worktrees.push(Worktree {
            path,
            head,
            branch: current_branch,
        });
    }

    worktrees
}

/// Get list of worktrees from git worktree list
pub fn get_worktree_list(hub_root: &Path) -> Result<Vec<Worktree>, GitError> {
    let output = run_git_in_dir(hub_root, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_list(&output))
}

/// Get the name of the current worktree based on the current directory
/// Returns None if not currently in a worktree (e.g., in the hub root)
pub fn get_current_worktree_name(hub_root: &Path) -> Result<Option<String>, GitError> {
    let current_dir = std::env::current_dir()
        .map_err(|e| GitError::new(format!("Cannot get current dir: {}", e)))?;

    let current_dir = current_dir
        .canonicalize()
        .unwrap_or_else(|_| current_dir.clone());

    let worktrees = get_worktree_list(hub_root)?;

    for wt in worktrees {
        // Skip the bare repo entry
        if wt.head == "(bare)" {
            continue;
        }

        let wt_path = wt.path.canonicalize().unwrap_or_else(|_| wt.path.clone());

        // Check if current dir is the worktree or inside it
        if current_dir.starts_with(&wt_path) {
            if let Some(name) = wt.path.file_name() {
                return Ok(Some(name.to_string_lossy().to_string()));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_list_empty() {
        let output = "";
        let result = parse_worktree_list(output);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_worktree_list_bare_only() {
        let output = "worktree /home/user/project/.bare\nbare\n";
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/home/user/project/.bare"));
        assert_eq!(result[0].head, "(bare)");
        assert!(result[0].branch.is_none());
    }

    #[test]
    fn test_parse_worktree_list_single_worktree() {
        let output =
            "worktree /home/user/project/main\nHEAD abc1234567890def\nbranch refs/heads/main\n";
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/home/user/project/main"));
        assert_eq!(result[0].head, "abc1234567890def");
        assert_eq!(result[0].branch, Some("refs/heads/main".to_string()));
    }

    #[test]
    fn test_parse_worktree_list_multiple_worktrees() {
        let output = "\
worktree /home/user/project/.bare
bare

worktree /home/user/project/main
HEAD abc1234567890def
branch refs/heads/main

worktree /home/user/project/feature
HEAD def4567890abc123
branch refs/heads/feature-branch
";
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 3);

        assert_eq!(result[0].path, PathBuf::from("/home/user/project/.bare"));
        assert_eq!(result[0].head, "(bare)");

        assert_eq!(result[1].path, PathBuf::from("/home/user/project/main"));
        assert_eq!(result[1].head, "abc1234567890def");
        assert_eq!(result[1].branch, Some("refs/heads/main".to_string()));

        assert_eq!(result[2].path, PathBuf::from("/home/user/project/feature"));
        assert_eq!(result[2].head, "def4567890abc123");
        assert_eq!(
            result[2].branch,
            Some("refs/heads/feature-branch".to_string())
        );
    }

    #[test]
    fn test_parse_worktree_list_detached_head() {
        let output = "worktree /home/user/project/detached\nHEAD abc1234567890def\n";
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/home/user/project/detached"));
        assert_eq!(result[0].head, "abc1234567890def");
        assert!(result[0].branch.is_none());
    }

    #[test]
    fn test_git_error_display() {
        let error = GitError::new("test error message");
        assert_eq!(format!("{}", error), "test error message");
    }

    #[test]
    fn test_git_error_new() {
        let error = GitError::new(String::from("owned string"));
        assert_eq!(error.message, "owned string");

        let error = GitError::new("static str");
        assert_eq!(error.message, "static str");
    }
}
