use crate::git::{find_hub_root, get_worktree_list};

/// Format branch information for display
/// - If branch is present, strips "refs/heads/" prefix
/// - If no branch (detached HEAD), shows first 7 chars of commit SHA
pub fn format_branch_info(branch: Option<&str>, head: &str) -> String {
    branch
        .map(|b| {
            // Strip refs/heads/ prefix if present
            b.strip_prefix("refs/heads/").unwrap_or(b).to_string()
        })
        .unwrap_or_else(|| head.chars().take(7).collect())
}

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

        let branch_info = format_branch_info(wt.branch.as_deref(), &wt.head);

        // Skip the bare repo entry (shown as .bare)
        if name == ".bare" {
            continue;
        }

        println!("{:<20} [{}]", name, branch_info);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_branch_info_with_refs_heads_prefix() {
        let result = format_branch_info(Some("refs/heads/main"), "abc1234");
        assert_eq!(result, "main");
    }

    #[test]
    fn test_format_branch_info_without_prefix() {
        let result = format_branch_info(Some("feature-branch"), "abc1234");
        assert_eq!(result, "feature-branch");
    }

    #[test]
    fn test_format_branch_info_detached_head() {
        let result = format_branch_info(None, "abc1234567890def");
        assert_eq!(result, "abc1234");
    }

    #[test]
    fn test_format_branch_info_short_sha() {
        let result = format_branch_info(None, "abc");
        assert_eq!(result, "abc");
    }

    #[test]
    fn test_format_branch_info_exact_7_chars() {
        let result = format_branch_info(None, "1234567");
        assert_eq!(result, "1234567");
    }

    #[test]
    fn test_format_branch_info_empty_head() {
        let result = format_branch_info(None, "");
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_branch_info_nested_refs() {
        // Branch names like refs/heads/feature/my-feature
        let result = format_branch_info(Some("refs/heads/feature/my-feature"), "abc1234");
        assert_eq!(result, "feature/my-feature");
    }

    #[test]
    fn test_format_branch_info_bare_marker() {
        // The (bare) marker from parse_worktree_list
        let result = format_branch_info(None, "(bare)");
        assert_eq!(result, "(bare)");
    }
}
