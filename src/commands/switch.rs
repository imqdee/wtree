use crate::git::{find_hub_root, get_current_worktree_name, get_worktree_list, GitError};
use crate::hooks::{load_hooks, run_post_hooks, run_pre_hooks, HookContext};
use crate::state::{read_previous_worktree, save_previous_worktree};

pub fn run(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;

    // Resolve "-" to the previous worktree name
    let target_name = if name == "-" {
        read_previous_worktree(&hub_root)?
            .ok_or_else(|| GitError::new("No previous worktree. Use 'wt switch <name>' first."))?
    } else {
        name.to_string()
    };

    let worktrees = get_worktree_list(&hub_root)?;

    // Get current worktree name before switching (for saving state)
    let current_worktree = get_current_worktree_name(&hub_root)?;

    // Find the worktree by name (matching the directory name)
    for wt in &worktrees {
        if let Some(dir_name) = wt.path.file_name() {
            if dir_name.to_string_lossy() == target_name {
                // Load and run pre-hooks
                let hooks = load_hooks(&hub_root);
                let context = HookContext::new("switch", &target_name, &wt.path, &hub_root, None);
                run_pre_hooks(&hooks, &context)?;

                // Save current worktree as previous (only if different from target)
                if let Some(ref current) = current_worktree {
                    if current != &target_name {
                        save_previous_worktree(&hub_root, current)?;
                    }
                }

                // Run post-hooks (from target worktree)
                run_post_hooks(&hooks, &context);

                // Print path for shell wrapper to cd into
                println!("{}", wt.path.display());
                return Ok(());
            }
        }
    }

    Err(Box::new(GitError::new(format!(
        "Worktree '{}' not found. Use 'wt list' to see available worktrees.",
        target_name
    ))))
}
