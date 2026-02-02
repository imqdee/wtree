use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::git::GitError;

/// Template content for hooks.toml with commented examples
const HOOKS_TEMPLATE: &str = r#"# wtree hooks configuration
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
fn get_global_default_hooks_path() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(|home| {
        std::path::PathBuf::from(home)
            .join(".wtree")
            .join("default-hooks.toml")
    })
}

/// Read global default hooks configuration if it exists
fn read_global_default_hooks() -> Option<String> {
    let path = get_global_default_hooks_path()?;
    std::fs::read_to_string(&path).ok()
}

/// Create .wtree directory with hooks.toml
/// Uses global default from ~/.wtree/default-hooks.toml if it exists,
/// otherwise uses the built-in template.
fn create_wtree_config(repo_dir: &Path) -> std::io::Result<()> {
    let wtree_dir = repo_dir.join(".wtree");
    fs::create_dir(&wtree_dir)?;

    let hooks_content = read_global_default_hooks().unwrap_or_else(|| HOOKS_TEMPLATE.to_string());

    fs::write(wtree_dir.join("hooks.toml"), hooks_content)?;
    Ok(())
}

/// Get the default branch name from a bare repository
fn get_default_branch(repo_dir: &Path) -> Option<String> {
    // For bare clones, HEAD points to the default branch
    // e.g., "ref: refs/heads/main"
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(["symbolic-ref", "HEAD"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        let ref_path = String::from_utf8_lossy(&output.stdout);
        // refs/heads/main -> main
        return ref_path
            .trim()
            .strip_prefix("refs/heads/")
            .map(|s| s.to_string());
    }

    None
}

/// Extract repository name from URL
/// Handles both HTTPS and SSH formats:
/// - https://github.com/user/my-repo.git -> my-repo
/// - git@github.com:user/my-repo.git -> my-repo
/// - https://github.com/user/my-repo -> my-repo
fn extract_repo_name(url: &str) -> Result<String, GitError> {
    let url = url.trim_end_matches('/');

    // Get the last path component
    let name = url
        .rsplit('/')
        .next()
        .or_else(|| url.rsplit(':').next())
        .ok_or_else(|| GitError::new("Cannot parse repository URL"))?;

    // Remove .git suffix if present
    let name = name.strip_suffix(".git").unwrap_or(name);

    if name.is_empty() {
        return Err(GitError::new("Cannot extract repository name from URL"));
    }

    Ok(name.to_string())
}

pub fn run(url: &str, switch: bool) -> Result<(), Box<dyn std::error::Error>> {
    let repo_name = extract_repo_name(url)?;
    let repo_dir = std::env::current_dir()?.join(&repo_name);

    if repo_dir.exists() {
        return Err(Box::new(GitError::new(format!(
            "Directory '{}' already exists",
            repo_name
        ))));
    }

    if !switch {
        println!("Cloning {} into {}/", url, repo_name);
    }

    // Create the repo directory
    fs::create_dir(&repo_dir)?;

    // Clone bare into .bare subdirectory
    let bare_path = repo_dir.join(".bare");
    let status = Command::new("git")
        .args(["clone", "--bare", url, bare_path.to_str().unwrap()])
        .stdout(if switch {
            Stdio::null()
        } else {
            Stdio::inherit()
        })
        .stderr(if switch {
            Stdio::null()
        } else {
            Stdio::inherit()
        })
        .status()?;

    if !status.success() {
        // Clean up on failure
        let _ = fs::remove_dir_all(&repo_dir);
        return Err(Box::new(GitError::new("Failed to clone repository")));
    }

    // Create .git file pointing to .bare
    let git_file = repo_dir.join(".git");
    fs::write(&git_file, "gitdir: ./.bare\n")?;

    // Create .wtree directory with template hooks.toml
    if let Err(e) = create_wtree_config(&repo_dir) {
        if !switch {
            eprintln!("Warning: Failed to create .wtree config: {}", e);
        }
    }

    // Configure the bare repo for proper fetch behavior
    // This ensures `git fetch` brings all branches properly
    let config_status = Command::new("git")
        .current_dir(&repo_dir)
        .args([
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ])
        .status()?;

    if !config_status.success() && !switch {
        eprintln!("Warning: Failed to configure fetch refspec");
    }

    // Detect and create worktree for default branch
    if let Some(default_branch) = get_default_branch(&repo_dir) {
        // When running from repo_dir, worktree path is just the branch name
        let worktree_status = Command::new("git")
            .current_dir(&repo_dir)
            .args(["worktree", "add", &default_branch, &default_branch])
            .stdout(if switch {
                Stdio::null()
            } else {
                Stdio::inherit()
            })
            .stderr(if switch {
                Stdio::null()
            } else {
                Stdio::inherit()
            })
            .status()?;

        if worktree_status.success() {
            if switch {
                // Print only the path for shell wrapper to cd into
                println!("{}", repo_dir.join(&default_branch).display());
            } else {
                println!("Created bare repository at {}/", repo_name);
                println!(
                    "Created worktree '{}' at {}/{}/",
                    default_branch, repo_name, default_branch
                );
                println!("Use 'cd {}/{}' to start working", repo_name, default_branch);
            }
        } else if switch {
            // Fallback to repo root if worktree creation failed
            println!("{}", repo_dir.display());
        } else {
            println!("Created bare repository at {}/", repo_name);
            eprintln!("Warning: Failed to create default branch worktree");
            println!(
                "Use 'cd {}' then 'wt create <name>' to create a worktree",
                repo_name
            );
        }
    } else if switch {
        // No default branch detected, switch to repo root
        println!("{}", repo_dir.display());
    } else {
        println!("Created bare repository at {}/", repo_name);
        println!(
            "Use 'cd {}' then 'wt create <name>' to create a worktree",
            repo_name
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_repo_name_https() {
        assert_eq!(
            extract_repo_name("https://github.com/user/my-repo.git").unwrap(),
            "my-repo"
        );
    }

    #[test]
    fn test_extract_repo_name_https_no_git() {
        assert_eq!(
            extract_repo_name("https://github.com/user/my-repo").unwrap(),
            "my-repo"
        );
    }

    #[test]
    fn test_extract_repo_name_ssh() {
        assert_eq!(
            extract_repo_name("git@github.com:user/my-repo.git").unwrap(),
            "my-repo"
        );
    }

    #[test]
    fn test_extract_repo_name_trailing_slash() {
        assert_eq!(
            extract_repo_name("https://github.com/user/my-repo/").unwrap(),
            "my-repo"
        );
    }

    #[test]
    fn test_extract_repo_name_ssh_no_git_suffix() {
        assert_eq!(
            extract_repo_name("git@github.com:user/my-repo").unwrap(),
            "my-repo"
        );
    }

    #[test]
    fn test_extract_repo_name_gitlab() {
        assert_eq!(
            extract_repo_name("https://gitlab.com/user/project.git").unwrap(),
            "project"
        );
    }

    #[test]
    fn test_extract_repo_name_bitbucket() {
        assert_eq!(
            extract_repo_name("git@bitbucket.org:team/repository.git").unwrap(),
            "repository"
        );
    }

    #[test]
    fn test_extract_repo_name_self_hosted() {
        assert_eq!(
            extract_repo_name("https://git.company.com/team/internal-tool.git").unwrap(),
            "internal-tool"
        );
    }

    #[test]
    fn test_extract_repo_name_with_dashes_and_underscores() {
        assert_eq!(
            extract_repo_name("https://github.com/user/my_awesome-repo.git").unwrap(),
            "my_awesome-repo"
        );
    }

    #[test]
    fn test_extract_repo_name_nested_path() {
        assert_eq!(
            extract_repo_name("https://gitlab.com/group/subgroup/project.git").unwrap(),
            "project"
        );
    }

    #[test]
    fn test_extract_repo_name_multiple_trailing_slashes() {
        assert_eq!(
            extract_repo_name("https://github.com/user/repo///").unwrap(),
            "repo"
        );
    }

    #[test]
    fn test_extract_repo_name_only_git_suffix() {
        // URL ending with just ".git" should fail
        let result = extract_repo_name("https://github.com/.git");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_repo_name_empty_string() {
        let result = extract_repo_name("");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_repo_name_only_slashes() {
        let result = extract_repo_name("///");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_global_default_hooks_path() {
        std::env::set_var("HOME", "/home/testuser");
        let path = get_global_default_hooks_path();
        assert!(path.is_some());
        assert_eq!(
            path.unwrap().to_str().unwrap(),
            "/home/testuser/.wtree/default-hooks.toml"
        );
    }

    #[test]
    fn test_read_global_default_hooks_missing() {
        std::env::set_var("HOME", "/nonexistent/path");
        let content = read_global_default_hooks();
        assert!(content.is_none());
    }
}
