use crate::git::{find_hub_root, get_worktree_list, GitError};

pub fn run(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;
    let worktrees = get_worktree_list(&hub_root)?;

    // Find the worktree by name (matching the directory name)
    for wt in &worktrees {
        if let Some(dir_name) = wt.path.file_name() {
            if dir_name.to_string_lossy() == name {
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
