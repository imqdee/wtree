use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::git::{
    self, detect_repo, get_current_worktree_name, get_worktree_list, run_git_in_dir, GitError,
    Worktree,
};
use crate::hooks::{load_hooks, run_post_hooks, run_pre_hooks, HookContext};

/// True when two paths point at the same location (canonicalized when possible).
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Worktrees to prune as `(name, absolute_path)` pairs.
///
/// The keep-set depends on the layout:
/// - Bare (`main_worktree == None`): keep the bare entry and the
///   `default_branch` worktree; prune the rest.
/// - Standard (`main_worktree == Some`): keep only the main worktree (matched by
///   path); every linked worktree is prunable, regardless of its branch. The
///   primary checkout's current branch is NOT a reliable repository default, so
///   `default_branch` is unused here.
///
/// The absolute path is carried through so removal never re-resolves by final
/// path component (which could pick the wrong worktree on a basename collision).
pub fn get_worktrees_to_prune(
    worktrees: &[Worktree],
    default_branch: Option<&str>,
    main_worktree: Option<&Path>,
) -> Vec<(String, PathBuf)> {
    let default_ref = default_branch.map(|b| format!("refs/heads/{}", b));

    worktrees
        .iter()
        .filter(|wt| {
            // Keep the bare repo entry
            if wt.head == "(bare)" {
                return false;
            }
            // Standard mode: keep only the main worktree, prune all linked ones.
            if let Some(main) = main_worktree {
                return !same_path(&wt.path, main);
            }
            // Bare mode: keep the default branch worktree.
            if let Some(ref default_ref) = default_ref {
                if wt.branch.as_deref() == Some(default_ref.as_str()) {
                    return false;
                }
            }
            true
        })
        .filter_map(|wt| {
            wt.path
                .file_name()
                .map(|n| (n.to_string_lossy().to_string(), wt.path.clone()))
        })
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
    let ctx = detect_repo()?;
    let anchor = ctx.anchor_dir().to_path_buf();
    let worktrees = get_worktree_list(&anchor)?;

    // Bare mode keeps the default-branch worktree, so it needs the default
    // branch. Standard mode keeps the main worktree by path and does not.
    let default_branch = git::get_default_branch(&anchor);
    if !ctx.is_standard() && default_branch.is_none() {
        return Err(Box::new(GitError::new(
            "Cannot determine default branch. Is this a valid bare repository?",
        )));
    }

    let default_for_filter = if ctx.is_standard() {
        None
    } else {
        default_branch.as_deref()
    };
    let targets = get_worktrees_to_prune(&worktrees, default_for_filter, ctx.main_worktree());

    if targets.is_empty() {
        if ctx.is_standard() {
            println!("Nothing to prune. Only the main worktree exists.");
        } else {
            println!(
                "Nothing to prune. Only the default worktree '{}' exists.",
                default_branch.as_deref().unwrap_or("")
            );
        }
        return Ok(());
    }

    let names: Vec<String> = targets.iter().map(|(n, _)| n.clone()).collect();

    // Safety: refuse to prune while inside a worktree that would be removed.
    if let Some(current) = get_current_worktree_name(&anchor)? {
        if names.contains(&current) {
            let dest = if ctx.is_standard() {
                "the main worktree".to_string()
            } else {
                format!("'{}'", default_branch.as_deref().unwrap_or(""))
            };
            return Err(Box::new(GitError::new(format!(
                "Cannot prune while inside worktree '{}'. Switch to {} first.",
                current, dest
            ))));
        }
    }

    println!("{}", format_prune_list(&names));

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

    let hooks = load_hooks(&ctx);
    let mut errors: Vec<(String, String)> = Vec::new();
    let mut removed = 0;

    for (name, worktree_path) in &targets {
        let context = HookContext::new("remove", name, worktree_path, &anchor, None);

        if let Err(e) = run_pre_hooks(&hooks, &context) {
            errors.push((name.clone(), e.to_string()));
            continue;
        }

        let wt_path = worktree_path.to_string_lossy();
        match run_git_in_dir(&anchor, &["worktree", "remove", wt_path.as_ref()]) {
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

    fn make_worktree(path: &str, head: &str, branch: Option<&str>) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            head: head.to_string(),
            branch: branch.map(|s| s.to_string()),
        }
    }

    /// Extract just the names from the prune targets for assertions.
    fn names_of(targets: &[(String, PathBuf)]) -> Vec<String> {
        targets.iter().map(|(n, _)| n.clone()).collect()
    }

    #[test]
    fn test_get_worktrees_to_prune_filters_default() {
        let worktrees = vec![
            make_worktree("/project/.bare", "(bare)", None),
            make_worktree("/project/main", "abc123", Some("refs/heads/main")),
            make_worktree("/project/feature-a", "def456", Some("refs/heads/feature-a")),
            make_worktree("/project/feature-b", "789abc", Some("refs/heads/feature-b")),
        ];
        let result = get_worktrees_to_prune(&worktrees, Some("main"), None);
        assert_eq!(names_of(&result), vec!["feature-a", "feature-b"]);
    }

    #[test]
    fn test_get_worktrees_to_prune_empty_when_only_default() {
        let worktrees = vec![
            make_worktree("/project/.bare", "(bare)", None),
            make_worktree("/project/main", "abc123", Some("refs/heads/main")),
        ];
        let result = get_worktrees_to_prune(&worktrees, Some("main"), None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_worktrees_to_prune_handles_detached_head() {
        let worktrees = vec![
            make_worktree("/project/.bare", "(bare)", None),
            make_worktree("/project/main", "abc123", Some("refs/heads/main")),
            make_worktree("/project/detached", "def456", None),
        ];
        let result = get_worktrees_to_prune(&worktrees, Some("main"), None);
        assert_eq!(names_of(&result), vec!["detached"]);
    }

    #[test]
    fn test_get_worktrees_to_prune_works_with_non_main_default() {
        let worktrees = vec![
            make_worktree("/project/.bare", "(bare)", None),
            make_worktree("/project/develop", "abc123", Some("refs/heads/develop")),
            make_worktree("/project/feature", "def456", Some("refs/heads/feature")),
        ];
        let result = get_worktrees_to_prune(&worktrees, Some("develop"), None);
        assert_eq!(names_of(&result), vec!["feature"]);
    }

    #[test]
    fn test_get_worktrees_to_prune_excludes_standard_main_off_default_branch() {
        // Standard mode: main worktree is on a non-default branch. It must never
        // be pruned, identified by path rather than branch.
        let worktrees = vec![
            make_worktree("/project", "abc123", Some("refs/heads/feature-x")),
            make_worktree(
                "/project/.claude/worktrees/feat",
                "def456",
                Some("refs/heads/feat"),
            ),
        ];
        let result = get_worktrees_to_prune(&worktrees, None, Some(Path::new("/project")));
        assert_eq!(names_of(&result), vec!["feat"]);
    }

    #[test]
    fn test_standard_prunes_linked_default_branch_keeps_only_main() {
        // P1 regression guard: main on a feature branch, a linked worktree on the
        // repo's "main" branch. Standard mode keeps only the main worktree (by
        // path); the linked worktree is prunable regardless of its branch, and
        // the main worktree is never removed.
        let worktrees = vec![
            make_worktree("/project", "abc123", Some("refs/heads/feature-x")),
            make_worktree(
                "/project/.claude/worktrees/mainwt",
                "def456",
                Some("refs/heads/main"),
            ),
        ];
        let result = get_worktrees_to_prune(&worktrees, None, Some(Path::new("/project")));
        assert_eq!(names_of(&result), vec!["mainwt"]);
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
