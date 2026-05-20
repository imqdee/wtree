use std::path::Path;

use crate::git::{GitError, RepoContext};

/// Normalize a gitignore line for equivalence comparison: trim whitespace, drop
/// a leading `./`, drop any trailing `/`. So `.claude/worktrees`,
/// `./.claude/worktrees/`, and `.claude/worktrees/` all normalize to the same
/// value, which is what makes the "exactly once" guarantee robust.
fn normalize_entry(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix("./").unwrap_or(t);
    t.trim_end_matches('/').to_string()
}

/// Resolve the gitignore entry this context should manage, as
/// `(gitignore_path, canonical_entry, normalized_entry)`.
///
/// Returns `None` (nothing to manage) when:
/// - the layout is bare (worktrees live in the hub root, not a tracked tree), or
/// - `worktree_base` resolves outside the main worktree (no local path to ignore), or
/// - the resolved relative path is empty (degenerate, e.g. base == main worktree).
fn entry_for(ctx: &RepoContext) -> Option<(std::path::PathBuf, String, String)> {
    let main = ctx.main_worktree()?;
    let base = ctx.worktree_base();
    let rel = base.strip_prefix(main).ok()?;
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let normalized = normalize_entry(&rel_str);
    if normalized.is_empty() {
        return None;
    }
    let entry = format!("{}/", normalized);
    Some((main.join(".gitignore"), entry, normalized))
}

/// Ensure the standard-mode `worktree_base` is ignored in the main worktree's
/// `.gitignore`, exactly once. Idempotent and safe to call from both `wt init`
/// and lazy `wt create`. No-op for bare layouts and for bases outside the
/// main worktree.
pub fn ensure_gitignore_entry(ctx: &RepoContext) -> Result<(), GitError> {
    let Some((gitignore_path, entry, normalized)) = entry_for(ctx) else {
        return Ok(());
    };

    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();

    for line in existing.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if normalize_entry(l) == normalized {
            return Ok(()); // already present, leave the file untouched
        }
    }

    let mut new_content = existing;
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(&entry);
    new_content.push('\n');

    write_gitignore(&gitignore_path, &new_content)
}

fn write_gitignore(path: &Path, content: &str) -> Result<(), GitError> {
    std::fs::write(path, content)
        .map_err(|e| GitError::new(format!("Failed to write {}: {}", path.display(), e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Layout;
    use std::fs;
    use tempfile::TempDir;

    fn standard_ctx(tmp: &TempDir) -> RepoContext {
        let common_dir = tmp.path().join(".git");
        fs::create_dir_all(&common_dir).unwrap();
        RepoContext {
            layout: Layout::Standard {
                main_worktree: tmp.path().to_path_buf(),
                common_dir,
            },
        }
    }

    #[test]
    fn test_normalize_entry_variants() {
        assert_eq!(normalize_entry(".claude/worktrees"), ".claude/worktrees");
        assert_eq!(normalize_entry(".claude/worktrees/"), ".claude/worktrees");
        assert_eq!(normalize_entry("./.claude/worktrees/"), ".claude/worktrees");
        assert_eq!(
            normalize_entry("  .claude/worktrees/  "),
            ".claude/worktrees"
        );
    }

    #[test]
    fn test_missing_file_is_created() {
        let tmp = TempDir::new().unwrap();
        let ctx = standard_ctx(&tmp);
        ensure_gitignore_entry(&ctx).unwrap();
        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(content, ".claude/worktrees/\n");
    }

    #[test]
    fn test_appends_when_absent() {
        let tmp = TempDir::new().unwrap();
        let ctx = standard_ctx(&tmp);
        fs::write(tmp.path().join(".gitignore"), "/target\nCargo.lock\n").unwrap();
        ensure_gitignore_entry(&ctx).unwrap();
        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(content, "/target\nCargo.lock\n.claude/worktrees/\n");
    }

    #[test]
    fn test_no_trailing_newline_gets_one() {
        let tmp = TempDir::new().unwrap();
        let ctx = standard_ctx(&tmp);
        fs::write(tmp.path().join(".gitignore"), "/target").unwrap(); // no newline
        ensure_gitignore_entry(&ctx).unwrap();
        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(content, "/target\n.claude/worktrees/\n");
    }

    #[test]
    fn test_idempotent_exact_entry() {
        let tmp = TempDir::new().unwrap();
        let ctx = standard_ctx(&tmp);
        let before = "/target\n.claude/worktrees/\n";
        fs::write(tmp.path().join(".gitignore"), before).unwrap();
        ensure_gitignore_entry(&ctx).unwrap();
        let after = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(after, before, "must be byte-identical when already present");
    }

    #[test]
    fn test_idempotent_trailing_slash_variant() {
        let tmp = TempDir::new().unwrap();
        let ctx = standard_ctx(&tmp);
        // Present without trailing slash; must still be treated as present.
        let before = "/target\n.claude/worktrees\n";
        fs::write(tmp.path().join(".gitignore"), before).unwrap();
        ensure_gitignore_entry(&ctx).unwrap();
        let after = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(
            after, before,
            "trailing-slash variant must count as present"
        );
    }

    #[test]
    fn test_idempotent_dot_slash_variant() {
        let tmp = TempDir::new().unwrap();
        let ctx = standard_ctx(&tmp);
        let before = "./.claude/worktrees/\n";
        fs::write(tmp.path().join(".gitignore"), before).unwrap();
        ensure_gitignore_entry(&ctx).unwrap();
        let after = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(after, before, "./ variant must count as present");
    }

    #[test]
    fn test_bare_layout_is_noop() {
        let tmp = TempDir::new().unwrap();
        let ctx = RepoContext {
            layout: Layout::Bare {
                hub_root: tmp.path().to_path_buf(),
            },
        };
        ensure_gitignore_entry(&ctx).unwrap();
        assert!(!tmp.path().join(".gitignore").exists());
    }

    #[test]
    fn test_worktree_base_outside_main_is_noop() {
        let tmp = TempDir::new().unwrap();
        let ctx = standard_ctx(&tmp);
        // Point worktree_base at an absolute path outside the main worktree.
        let state_dir = tmp.path().join(".git/wtree");
        fs::create_dir_all(&state_dir).unwrap();
        let outside = TempDir::new().unwrap();
        fs::write(
            state_dir.join("config.toml"),
            format!("worktree_base = \"{}\"\n", outside.path().display()),
        )
        .unwrap();

        ensure_gitignore_entry(&ctx).unwrap();
        assert!(
            !tmp.path().join(".gitignore").exists(),
            "outside-worktree base must not touch .gitignore"
        );
    }
}
