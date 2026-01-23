use crate::git::{find_hub_root, get_worktree_list};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;
    let worktrees = get_worktree_list(&hub_root)?;

    if worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    for wt in worktrees {
        let name = wt
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| wt.path.display().to_string());

        let branch_info = wt
            .branch
            .as_ref()
            .map(|b| {
                // Strip refs/heads/ prefix if present
                b.strip_prefix("refs/heads/")
                    .unwrap_or(b)
                    .to_string()
            })
            .unwrap_or_else(|| wt.head.chars().take(7).collect());

        // Skip the bare repo entry (shown as .bare)
        if name == ".bare" {
            continue;
        }

        println!("{:<20} [{}]", name, branch_info);
    }

    Ok(())
}
