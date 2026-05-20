use crate::git::{detect_repo, get_current_worktree_name, get_worktree_list, run_git_in_dir};
use crate::gitignore::ensure_gitignore_entry;
use crate::hooks::{load_hooks, run_post_hooks, run_pre_hooks, HookContext};
use crate::state::save_previous_worktree;

pub fn run(
    name: &str,
    checkout: Option<&str>,
    base: Option<&str>,
    switch: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = detect_repo()?;
    let anchor = ctx.anchor_dir().to_path_buf();
    let worktree_base = ctx.worktree_base();
    let worktree_path = worktree_base.join(name);

    // Standard-mode lazy init: ensure the worktree parent dir exists, the state
    // dir exists, and worktree_base is gitignored. No-op in bare mode.
    if ctx.is_standard() {
        std::fs::create_dir_all(&worktree_base)?;
        std::fs::create_dir_all(ctx.state_dir())?;
        ensure_gitignore_entry(&ctx)?;
    }

    // Get current worktree name before creating (for saving state when switching)
    let current_worktree = if switch {
        get_current_worktree_name(&anchor)?
    } else {
        None
    };

    // Resolve --base to the source worktree's HEAD SHA
    let base_sha: Option<String> = if let Some(source) = base {
        let worktrees = get_worktree_list(&anchor)?;
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
    let hooks = load_hooks(&ctx);
    let ctx_branch = checkout.or(base.map(|_| name));
    let context = HookContext::new("create", name, &worktree_path, &anchor, ctx_branch);
    run_pre_hooks(&hooks, &context)?;

    // Pass the resolved absolute path to git worktree add rather than relying on
    // cwd-relative resolution (which only matches the bare layout's sibling dirs).
    let wt_path = worktree_path.to_string_lossy();
    let args: Vec<&str> = match (checkout, base_sha.as_deref()) {
        (Some(b), _) => vec!["worktree", "add", wt_path.as_ref(), b],
        (_, Some(sha)) => vec!["worktree", "add", "-b", name, wt_path.as_ref(), sha],
        (None, None) => vec!["worktree", "add", wt_path.as_ref()],
    };

    run_git_in_dir(&anchor, &args)?;

    // Run post-hooks (from worktree directory)
    run_post_hooks(&hooks, &context);

    if switch {
        // Save current worktree as previous (if we were in a worktree)
        if let Some(ref current) = current_worktree {
            save_previous_worktree(&ctx, current)?;
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
