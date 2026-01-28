use crate::git::{find_hub_root, run_git_in_dir};
use crate::hooks::{load_hooks, run_post_hooks, run_pre_hooks, HookContext};

/// Format the error summary message for failed removals
pub fn format_error_summary(error_count: usize) -> String {
    format!("{} worktree(s) could not be removed", error_count)
}

/// Format a single error line for display
pub fn format_error_line(name: &str, err: &str) -> String {
    format!("  - '{}': {}", name, err)
}

pub fn run(names: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let hub_root = find_hub_root()?;
    let hooks = load_hooks(&hub_root);
    let mut errors: Vec<(&str, String)> = Vec::new();

    for name in names {
        let worktree_path = hub_root.join(name);
        let context = HookContext::new("remove", name, &worktree_path, &hub_root, None);

        // Run pre-hooks; if they fail, skip this worktree
        if let Err(e) = run_pre_hooks(&hooks, &context) {
            errors.push((name, e.to_string()));
            continue;
        }

        match run_git_in_dir(&hub_root, &["worktree", "remove", name]) {
            Ok(_) => {
                // Run post-hooks (from hub root, worktree is gone)
                run_post_hooks(&hooks, &context);
                println!("Removed worktree '{}'", name);
            }
            Err(e) => errors.push((name, e.to_string())),
        }
    }

    if !errors.is_empty() {
        eprintln!("\nFailed to remove {} worktree(s):", errors.len());
        for (name, err) in &errors {
            eprintln!("{}", format_error_line(name, err));
        }
        return Err(format_error_summary(errors.len()).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_error_summary_single() {
        let result = format_error_summary(1);
        assert_eq!(result, "1 worktree(s) could not be removed");
    }

    #[test]
    fn test_format_error_summary_multiple() {
        let result = format_error_summary(3);
        assert_eq!(result, "3 worktree(s) could not be removed");
    }

    #[test]
    fn test_format_error_line() {
        let result = format_error_line("my-worktree", "fatal: not a working tree");
        assert_eq!(result, "  - 'my-worktree': fatal: not a working tree");
    }

    #[test]
    fn test_format_error_line_with_special_chars() {
        let result = format_error_line("feature/test-123", "some error message");
        assert_eq!(result, "  - 'feature/test-123': some error message");
    }

    #[test]
    fn test_format_error_line_empty_name() {
        let result = format_error_line("", "error");
        assert_eq!(result, "  - '': error");
    }
}
