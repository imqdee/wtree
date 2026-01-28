use crate::git::{find_hub_root, run_git_in_dir};
use crate::hooks::{load_hooks, run_post_hooks, run_pre_hooks, HookContext};

pub fn run(
    name: &str,
    branch: Option<&str>,
    switch: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;
    let worktree_path = hub_root.join(name);

    // Load and run pre-hooks
    let hooks = load_hooks(&hub_root);
    let context = HookContext::new("create", name, &worktree_path, &hub_root, branch);
    run_pre_hooks(&hooks, &context)?;

    let args: Vec<&str> = match branch {
        Some(b) => vec!["worktree", "add", name, b],
        None => vec!["worktree", "add", name],
    };

    run_git_in_dir(&hub_root, &args)?;

    // Run post-hooks (from worktree directory)
    run_post_hooks(&hooks, &context);

    if switch {
        // Print only the path for shell wrapper to cd into
        println!("{}", worktree_path.display());
    } else {
        println!("Created worktree '{}' at {}", name, worktree_path.display());
        if let Some(b) = branch {
            println!("Checked out branch: {}", b);
        }
    }

    Ok(())
}
