use std::path::{Path, PathBuf};

use crate::git::{detect_repo, get_worktree_list, run_git_in_dir, RepoContext, Worktree};
use crate::hooks::{load_hooks, run_post_hooks, run_pre_hooks, HookContext};

/// Format the error summary message for failed removals
pub fn format_error_summary(error_count: usize) -> String {
    format!("{} worktree(s) could not be removed", error_count)
}

/// Format a single error line for display
pub fn format_error_line(name: &str, err: &str) -> String {
    format!("  - '{}': {}", name, err)
}

/// True when two paths point at the same location (canonicalized when possible).
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Resolve a worktree name to its absolute path.
///
/// Resolution is path-first: the managed location `worktree_base/<name>` is
/// matched against the registered worktree paths, which handles nested names
/// like `feature/foo` and disambiguates worktrees that share a final component.
/// If no managed worktree matches, it falls back to matching the final path
/// component; an ambiguous fallback (more than one match) is rejected rather
/// than silently removing the first hit.
fn resolve_worktree_path(
    worktrees: &[Worktree],
    ctx: &RepoContext,
    name: &str,
) -> Result<PathBuf, String> {
    // 1. Exact match against the managed path worktree_base/<name>.
    let candidate = ctx.worktree_base().join(name);
    let candidate_canon = candidate.canonicalize().ok();
    for wt in worktrees {
        let same = match (&candidate_canon, wt.path.canonicalize().ok()) {
            (Some(c), Some(p)) => *c == p,
            _ => wt.path == candidate,
        };
        if same {
            return Ok(wt.path.clone());
        }
    }

    // 2. Fall back to the final-path-component match (the bare-layout name).
    let matches: Vec<&Worktree> = worktrees
        .iter()
        .filter(|wt| {
            wt.path
                .file_name()
                .map(|n| n.to_string_lossy() == name)
                .unwrap_or(false)
        })
        .collect();

    match matches.len() {
        0 => Err(format!("Worktree '{}' not found", name)),
        1 => Ok(matches[0].path.clone()),
        n => Err(format!(
            "Worktree name '{}' is ambiguous ({} worktrees share that final path component); none is at the managed path {}",
            name,
            n,
            candidate.display()
        )),
    }
}

pub fn run(names: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = detect_repo()?;
    let anchor = ctx.anchor_dir().to_path_buf();
    let worktrees = get_worktree_list(&anchor)?;
    let hooks = load_hooks(&ctx);
    let mut errors: Vec<(&str, String)> = Vec::new();

    for name in names {
        // Resolve the worktree to its absolute path. Passing the bare name to
        // `git worktree remove` only resolves in the bare layout; in standard
        // mode the worktree lives under `.claude/worktrees/`.
        let worktree_path = match resolve_worktree_path(&worktrees, &ctx, name) {
            Ok(p) => p,
            Err(msg) => {
                errors.push((name, msg));
                continue;
            }
        };

        // Never remove the standard-mode main worktree.
        if let Some(main) = ctx.main_worktree() {
            if same_path(&worktree_path, main) {
                errors.push((name, "refusing to remove the main worktree".to_string()));
                continue;
            }
        }

        let context = HookContext::new("remove", name, &worktree_path, &anchor, None);

        // Run pre-hooks; if they fail, skip this worktree
        if let Err(e) = run_pre_hooks(&hooks, &context) {
            errors.push((name, e.to_string()));
            continue;
        }

        let wt_path = worktree_path.to_string_lossy();
        match run_git_in_dir(&anchor, &["worktree", "remove", wt_path.as_ref()]) {
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
    use crate::git::Layout;

    fn make_worktree(path: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            head: "abc123".to_string(),
            branch: None,
        }
    }

    fn bare_ctx(hub_root: &str) -> RepoContext {
        RepoContext {
            layout: Layout::Bare {
                hub_root: PathBuf::from(hub_root),
            },
        }
    }

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

    #[test]
    fn test_resolve_simple_name() {
        // Non-canonicalizable paths exercise the fallback branch.
        let worktrees = vec![make_worktree("/hub/feat"), make_worktree("/hub/other")];
        let ctx = bare_ctx("/hub");
        let path = resolve_worktree_path(&worktrees, &ctx, "feat").unwrap();
        assert_eq!(path, PathBuf::from("/hub/feat"));
    }

    #[test]
    fn test_resolve_nested_name_by_path() {
        // `feature/foo` must resolve even though its final component is `foo`.
        let worktrees = vec![make_worktree("/hub/feature/foo"), make_worktree("/hub/bar")];
        let ctx = bare_ctx("/hub");
        let path = resolve_worktree_path(&worktrees, &ctx, "feature/foo").unwrap();
        assert_eq!(path, PathBuf::from("/hub/feature/foo"));
    }

    #[test]
    fn test_resolve_missing_name() {
        let worktrees = vec![make_worktree("/hub/feat")];
        let ctx = bare_ctx("/hub");
        let err = resolve_worktree_path(&worktrees, &ctx, "nope").unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_resolve_ambiguous_basename_rejected() {
        // Two worktrees share the final component `foo`, neither at the managed
        // path: the resolver must refuse rather than guess.
        let worktrees = vec![
            make_worktree("/elsewhere/a/foo"),
            make_worktree("/elsewhere/b/foo"),
        ];
        let ctx = bare_ctx("/hub");
        let err = resolve_worktree_path(&worktrees, &ctx, "foo").unwrap_err();
        assert!(err.contains("ambiguous"));
    }
}
