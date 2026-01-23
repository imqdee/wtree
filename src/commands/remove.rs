use crate::git::{find_hub_root, run_git_in_dir};

pub fn run(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;

    run_git_in_dir(&hub_root, &["worktree", "remove", name])?;

    println!("Removed worktree '{}'", name);

    Ok(())
}
