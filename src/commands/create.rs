use crate::git::{find_hub_root, run_git_in_dir};

pub fn run(name: &str, branch: Option<&str>, switch: bool) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;
    let worktree_path = hub_root.join(name);

    let args: Vec<&str> = match branch {
        Some(b) => vec!["worktree", "add", name, b],
        None => vec!["worktree", "add", name],
    };

    run_git_in_dir(&hub_root, &args)?;

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
