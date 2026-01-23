use std::fs;

use crate::git::{find_hub_root, get_worktree_list, GitError};

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
                            if file_name_str.starts_with(".env")
                                && file_name_str != ".env.example"
                                && entry.path().is_file()
                            {
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
