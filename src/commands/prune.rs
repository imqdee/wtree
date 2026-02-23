use std::io::{self, BufRead, Write};

use crate::git::{
    self, find_hub_root, get_current_worktree_name, get_worktree_list, run_git_in_dir, GitError,
    Worktree,
};
use crate::hooks::{load_hooks, run_post_hooks, run_pre_hooks, HookContext};

/// Given a list of worktrees and the default branch name, return the names
/// of worktrees that should be pruned (everything except bare and default).
pub fn get_worktrees_to_prune(worktrees: &[Worktree], default_branch: &str) -> Vec<String> {
    let default_ref = format!("refs/heads/{}", default_branch);

    worktrees
        .iter()
        .filter(|wt| {
            // Keep the bare repo entry
            if wt.head == "(bare)" {
                return false;
            }
            // Keep the default branch worktree
            if wt.branch.as_deref() == Some(default_ref.as_str()) {
                return false;
            }
            true
        })
        .filter_map(|wt| wt.path.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect()
}

/// Format a human-readable list of worktrees that will be pruned.
pub fn format_prune_list(names: &[String]) -> String {
    let mut lines = vec![format!(
        "The following {} worktree(s) will be removed:",
        names.len()
    )];
    for name in names {
        lines.push(format!("  - {}", name));
    }
    lines.join("\n")
}

/// Parse user input for yes/no confirmation.
/// Returns true for "y" or "yes" (case-insensitive).
pub fn confirm_prune(input: &str) -> bool {
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

pub fn run(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;

    let default_branch = git::get_default_branch(&hub_root).ok_or_else(|| {
        GitError::new("Cannot determine default branch. Is this a valid bare repository?")
    })?;

    let worktrees = get_worktree_list(&hub_root)?;
    let to_prune = get_worktrees_to_prune(&worktrees, &default_branch);

    if to_prune.is_empty() {
        println!(
            "Nothing to prune. Only the default worktree '{}' exists.",
            default_branch
        );
        return Ok(());
    }

    // Safety: refuse to prune while inside a non-default worktree that would be removed
    if let Some(current) = get_current_worktree_name(&hub_root)? {
        if to_prune.contains(&current) {
            return Err(Box::new(GitError::new(format!(
                "Cannot prune while inside non-default worktree '{}'. Switch to '{}' first.",
                current, default_branch
            ))));
        }
    }

    println!("{}", format_prune_list(&to_prune));

    if !force {
        print!("Continue? [y/N] ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;

        if !confirm_prune(&input) {
            println!("Aborted.");
            return Ok(());
        }
    }

    let hooks = load_hooks(&hub_root);
    let mut errors: Vec<(String, String)> = Vec::new();
    let mut removed = 0;

    for name in &to_prune {
        let worktree_path = hub_root.join(name);
        let context = HookContext::new("remove", name, &worktree_path, &hub_root, None);

        if let Err(e) = run_pre_hooks(&hooks, &context) {
            errors.push((name.clone(), e.to_string()));
            continue;
        }

        match run_git_in_dir(&hub_root, &["worktree", "remove", name]) {
            Ok(_) => {
                run_post_hooks(&hooks, &context);
                println!("Removed worktree '{}'", name);
                removed += 1;
            }
            Err(e) => errors.push((name.clone(), e.to_string())),
        }
    }

    if !errors.is_empty() {
        eprintln!("\nFailed to remove {} worktree(s):", errors.len());
        for (name, err) in &errors {
            eprintln!("  - '{}': {}", name, err);
        }
    }

    println!("\nPruned {} worktree(s).", removed);

    if !errors.is_empty() {
        return Err(format!("{} worktree(s) could not be removed", errors.len()).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_worktree(path: &str, head: &str, branch: Option<&str>) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            head: head.to_string(),
            branch: branch.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_get_worktrees_to_prune_filters_default() {
        let worktrees = vec![
            make_worktree("/project/.bare", "(bare)", None),
            make_worktree("/project/main", "abc123", Some("refs/heads/main")),
            make_worktree("/project/feature-a", "def456", Some("refs/heads/feature-a")),
            make_worktree("/project/feature-b", "789abc", Some("refs/heads/feature-b")),
        ];
        let result = get_worktrees_to_prune(&worktrees, "main");
        assert_eq!(result, vec!["feature-a", "feature-b"]);
    }

    #[test]
    fn test_get_worktrees_to_prune_empty_when_only_default() {
        let worktrees = vec![
            make_worktree("/project/.bare", "(bare)", None),
            make_worktree("/project/main", "abc123", Some("refs/heads/main")),
        ];
        let result = get_worktrees_to_prune(&worktrees, "main");
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_worktrees_to_prune_handles_detached_head() {
        let worktrees = vec![
            make_worktree("/project/.bare", "(bare)", None),
            make_worktree("/project/main", "abc123", Some("refs/heads/main")),
            make_worktree("/project/detached", "def456", None),
        ];
        let result = get_worktrees_to_prune(&worktrees, "main");
        assert_eq!(result, vec!["detached"]);
    }

    #[test]
    fn test_get_worktrees_to_prune_works_with_non_main_default() {
        let worktrees = vec![
            make_worktree("/project/.bare", "(bare)", None),
            make_worktree("/project/develop", "abc123", Some("refs/heads/develop")),
            make_worktree("/project/feature", "def456", Some("refs/heads/feature")),
        ];
        let result = get_worktrees_to_prune(&worktrees, "develop");
        assert_eq!(result, vec!["feature"]);
    }

    #[test]
    fn test_format_prune_list() {
        let names = vec!["feature-a".to_string(), "feature-b".to_string()];
        let result = format_prune_list(&names);
        assert_eq!(
            result,
            "The following 2 worktree(s) will be removed:\n  - feature-a\n  - feature-b"
        );
    }

    #[test]
    fn test_format_prune_list_single() {
        let names = vec!["old-branch".to_string()];
        let result = format_prune_list(&names);
        assert_eq!(
            result,
            "The following 1 worktree(s) will be removed:\n  - old-branch"
        );
    }

    #[test]
    fn test_confirm_prune_yes() {
        assert!(confirm_prune("y"));
        assert!(confirm_prune("Y"));
        assert!(confirm_prune("yes"));
        assert!(confirm_prune("YES"));
        assert!(confirm_prune("Yes"));
        assert!(confirm_prune("  y  "));
        assert!(confirm_prune("y\n"));
    }

    #[test]
    fn test_confirm_prune_no() {
        assert!(!confirm_prune("n"));
        assert!(!confirm_prune("no"));
        assert!(!confirm_prune("N"));
    }

    #[test]
    fn test_confirm_prune_empty() {
        assert!(!confirm_prune(""));
        assert!(!confirm_prune("  "));
        assert!(!confirm_prune("\n"));
    }
}
