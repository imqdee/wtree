use crate::git::{find_hub_root, get_current_worktree_name, get_worktree_list, run_git_in_dir};
use crate::hooks::{load_hooks, run_post_hooks, run_pre_hooks, HookContext};
use crate::state::save_previous_worktree;

pub fn run(
    name: &str,
    checkout: Option<&str>,
    base: Option<&str>,
    switch: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;
    let worktree_path = hub_root.join(name);

    // Get current worktree name before creating (for saving state when switching)
    let current_worktree = if switch {
        get_current_worktree_name(&hub_root)?
    } else {
        None
    };

    // Resolve --base to the source worktree's HEAD SHA
    let base_sha: Option<String> = if let Some(source) = base {
        let worktrees = get_worktree_list(&hub_root)?;
        let found = worktrees.iter().find(|wt| {
            wt.path
                .file_name()
                .map(|n| n.to_string_lossy() == source)
                .unwrap_or(false)
        });
        match found {
            Some(wt) => Some(wt.head.clone()),
            None => return Err(format!("Worktree '{}' not found", source).into()),
        }
    } else {
        None
    };

    // Load and run pre-hooks
    let hooks = load_hooks(&hub_root);
    let ctx_branch = checkout.or(base.map(|_| name));
    let context = HookContext::new("create", name, &worktree_path, &hub_root, ctx_branch);
    run_pre_hooks(&hooks, &context)?;

    let args: Vec<&str> = match (checkout, base_sha.as_deref()) {
        (Some(b), _) => vec!["worktree", "add", name, b],
        (_, Some(sha)) => vec!["worktree", "add", "-b", name, name, sha],
        (None, None) => vec!["worktree", "add", name],
    };

    run_git_in_dir(&hub_root, &args)?;

    // Run post-hooks (from worktree directory)
    run_post_hooks(&hooks, &context);

    if switch {
        // Save current worktree as previous (if we were in a worktree)
        if let Some(ref current) = current_worktree {
            save_previous_worktree(&hub_root, current)?;
        }
        // Print only the path for shell wrapper to cd into
        println!("{}", worktree_path.display());
    } else {
        println!("Created worktree '{}' at {}", name, worktree_path.display());
        if let Some(source) = base {
            println!("Branched from worktree: {}", source);
        } else if let Some(b) = checkout {
            println!("Checked out branch: {}", b);
        }
    }

    Ok(())
}
