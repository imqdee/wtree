use crate::git::{detect_repo, GitError, Layout};
use crate::gitignore::ensure_gitignore_entry;
use crate::hooks_template::default_hooks_content;

/// Adopt the current standard repository: create the state dir, drop a hooks
/// template, and ensure `worktree_base` is gitignored. Idempotent.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = detect_repo()?;

    let main_worktree = match &ctx.layout {
        Layout::Standard { main_worktree, .. } => main_worktree.clone(),
        Layout::Bare { .. } => {
            return Err(Box::new(GitError::new(
                "Already a wtree-managed bare repo. 'wt init' adopts a standard repo; use 'wt clone' to create a bare hub.",
            )));
        }
    };

    let state_dir = ctx.state_dir();
    std::fs::create_dir_all(&state_dir)
        .map_err(|e| GitError::new(format!("Failed to create state directory: {}", e)))?;

    // Write the hooks template only if absent, so re-running stays idempotent and
    // never clobbers a customized hooks.toml.
    let hooks_path = state_dir.join("hooks.toml");
    if !hooks_path.exists() {
        std::fs::write(&hooks_path, default_hooks_content())
            .map_err(|e| GitError::new(format!("Failed to write hooks.toml: {}", e)))?;
    }

    ensure_gitignore_entry(&ctx)?;

    let worktree_base = ctx.worktree_base();
    println!("Initialized wtree for standard repo.");
    println!("  main worktree: {}", main_worktree.display());
    println!("  state dir:     {}", state_dir.display());
    println!("  hooks:         {}", hooks_path.display());
    println!("  worktrees in:  {}", worktree_base.display());
    println!();
    println!(
        "Override placement with a `worktree_base` entry in {}.",
        state_dir.join("config.toml").display()
    );

    Ok(())
}
