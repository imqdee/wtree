use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct GitError {
    pub message: String,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GitError {}

impl GitError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Execute a git command in a specific directory
pub fn run_git_in_dir(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| GitError::new(format!("Failed to execute git: {}", e)))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(GitError::new(stderr))
    }
}

/// Walk up from `start` looking for the bare-hub layout (`.bare` dir, or a `.git`
/// file pointing at `./.bare`). Returns the hub root if found, `None` otherwise.
///
/// This is the canonical bare-layout probe used by `detect_repo`. It performs no
/// `git` invocation, so bare detection never depends on
/// `git rev-parse --show-toplevel` (which can fail in a bare hub that has no
/// normal checked-out toplevel).
fn find_bare_hub_root(start: &Path) -> Result<Option<PathBuf>, GitError> {
    let mut dir = start;
    loop {
        // Check if this directory contains .bare (hub root)
        let bare_path = dir.join(".bare");
        if bare_path.exists() && bare_path.is_dir() {
            return Ok(Some(dir.to_path_buf()));
        }

        // Check if there's a .git file (not directory) pointing to .bare. A
        // `.git` file that exists but cannot be read is a hard error, matching
        // the original `find_hub_root` (no silent fall-through to git detection).
        let git_path = dir.join(".git");
        if git_path.exists() && git_path.is_file() {
            let content = std::fs::read_to_string(&git_path)
                .map_err(|e| GitError::new(format!("Cannot read .git file: {}", e)))?;

            if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                let gitdir = gitdir.trim();
                let gitdir_path = if Path::new(gitdir).is_absolute() {
                    PathBuf::from(gitdir)
                } else {
                    dir.join(gitdir)
                };

                // The gitdir should point to .bare, so hub root is its parent
                if let Some(hub_root) = gitdir_path.parent() {
                    let hub_root = hub_root
                        .canonicalize()
                        .unwrap_or_else(|_| hub_root.to_path_buf());
                    if hub_root.join(".bare").exists() {
                        return Ok(Some(hub_root));
                    }
                }
            }
        }

        // Move up one directory
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return Ok(None),
        }
    }
}

/// Default subdirectory (relative to the main worktree) where standard-layout
/// worktrees are created when no `worktree_base` override is configured.
pub const DEFAULT_STANDARD_WORKTREE_SUBDIR: &str = ".claude/worktrees";

/// Repository layout. Determines where state, hooks, and worktrees live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    /// The original `<hub_root>/.bare` + sibling-worktrees layout.
    Bare { hub_root: PathBuf },
    /// A standard cloned repo. `main_worktree` is the primary checkout (never a
    /// linked worktree), `common_dir` is the shared `.git` directory.
    Standard {
        main_worktree: PathBuf,
        common_dir: PathBuf,
    },
}

/// Detected repository context. Every command routes path resolution through
/// this so bare and standard layouts share one code path. All paths held here
/// are absolute, canonicalized at detection time.
#[derive(Debug, Clone)]
pub struct RepoContext {
    pub layout: Layout,
}

impl RepoContext {
    /// Directory git commands run from (hub root for bare, main worktree for
    /// standard). Also the value exported to hooks as `WT_HUB_ROOT`.
    pub fn anchor_dir(&self) -> &Path {
        match &self.layout {
            Layout::Bare { hub_root } => hub_root,
            Layout::Standard { main_worktree, .. } => main_worktree,
        }
    }

    /// Directory holding `state` and `hooks.toml`.
    /// Bare: `<hub_root>/.wtree`. Standard: `<common_dir>/wtree` (i.e. `.git/wtree`).
    pub fn state_dir(&self) -> PathBuf {
        match &self.layout {
            Layout::Bare { hub_root } => hub_root.join(".wtree"),
            Layout::Standard { common_dir, .. } => common_dir.join("wtree"),
        }
    }

    /// Parent directory under which named worktrees are created;
    /// `worktree_base().join(name)` is the worktree path.
    /// Bare: `<hub_root>`. Standard: `<main_worktree>/.claude/worktrees` by
    /// default, or the `worktree_base` config override (relative resolves
    /// against the main worktree, absolute used as-is).
    pub fn worktree_base(&self) -> PathBuf {
        match &self.layout {
            Layout::Bare { hub_root } => hub_root.clone(),
            Layout::Standard { main_worktree, .. } => {
                match crate::config::load_config(self).worktree_base {
                    Some(base) => {
                        let pb = PathBuf::from(&base);
                        if pb.is_absolute() {
                            pb
                        } else {
                            main_worktree.join(pb)
                        }
                    }
                    None => main_worktree.join(DEFAULT_STANDARD_WORKTREE_SUBDIR),
                }
            }
        }
    }

    /// The main worktree path in standard mode, `None` in bare mode.
    pub fn main_worktree(&self) -> Option<&Path> {
        match &self.layout {
            Layout::Standard { main_worktree, .. } => Some(main_worktree),
            Layout::Bare { .. } => None,
        }
    }

    pub fn is_standard(&self) -> bool {
        matches!(self.layout, Layout::Standard { .. })
    }
}

/// Resolve `p` to an absolute, canonical path. Relative paths resolve against
/// `base`. Canonicalization falls back to the joined path when it fails (e.g.
/// the path does not exist yet).
fn absolutize(base: &Path, p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    let joined = if pb.is_absolute() { pb } else { base.join(pb) };
    joined.canonicalize().unwrap_or(joined)
}

/// Read the first `worktree` entry of `git worktree list --porcelain`, which git
/// always emits as the main (primary) worktree. Used to resolve the standard-mode
/// main worktree, since `git rev-parse --show-toplevel` returns the *current*
/// worktree (possibly a linked one), not the primary.
fn first_worktree_path(dir: &Path) -> Result<PathBuf, GitError> {
    let output = run_git_in_dir(dir, &["worktree", "list", "--porcelain"])?;
    for line in output.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            return Ok(absolutize(dir, p));
        }
    }
    Err(GitError::new(
        "Cannot determine main worktree from 'git worktree list'",
    ))
}

/// Detect the repository layout from the current directory.
pub fn detect_repo() -> Result<RepoContext, GitError> {
    let current_dir = std::env::current_dir()
        .map_err(|e| GitError::new(format!("Cannot get current dir: {}", e)))?;
    detect_repo_from(&current_dir)
}

/// Core detection, parameterized on the starting directory for testability.
///
/// Order is bare-first: the `.bare` probe runs before any `git` call so the
/// existing bare-hub layout is detected exactly as before, with no behavior
/// change. Only a non-bare-hub directory falls through to `git rev-parse`.
fn detect_repo_from(start: &Path) -> Result<RepoContext, GitError> {
    // 1. Bare-hub layout (no git invocation in the common case).
    if let Some(hub_root) = find_bare_hub_root(start)? {
        return Ok(RepoContext {
            layout: Layout::Bare { hub_root },
        });
    }

    // 2. Ask git. A failure here means we are not inside a repo at all.
    let common_dir_raw =
        run_git_in_dir(start, &["rev-parse", "--git-common-dir"]).map_err(|_| {
            GitError::new("Not inside a wtree repository (cannot find .bare directory)")
        })?;
    let common_dir = absolutize(start, &common_dir_raw);

    let is_bare = run_git_in_dir(start, &["rev-parse", "--is-bare-repository"])
        .map(|s| s == "true")
        .unwrap_or(false);

    if is_bare {
        // A bare repo that does not use our `.bare` hub convention. Treat the
        // parent of the git dir as the hub root.
        let hub_root = common_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| common_dir.clone());
        return Ok(RepoContext {
            layout: Layout::Bare { hub_root },
        });
    }

    // 3. Standard repo. Resolve the primary worktree from the porcelain list,
    // never from raw --show-toplevel (which yields the current worktree).
    let main_worktree = first_worktree_path(start)?;
    Ok(RepoContext {
        layout: Layout::Standard {
            main_worktree,
            common_dir,
        },
    })
}

/// Worktree information
#[derive(Debug)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
}

/// Parse git worktree list --porcelain output into structured data
pub fn parse_worktree_list(output: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            // Save previous worktree if exists
            if let (Some(path), Some(head)) = (current_path.take(), current_head.take()) {
                worktrees.push(Worktree {
                    path,
                    head,
                    branch: current_branch.take(),
                });
            }
            current_path = Some(PathBuf::from(path));
            current_branch = None;
        } else if let Some(head) = line.strip_prefix("HEAD ") {
            current_head = Some(head.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(branch.to_string());
        } else if line == "bare" {
            // Mark bare repo
            current_head = Some("(bare)".to_string());
        }
    }

    // Don't forget the last one
    if let (Some(path), Some(head)) = (current_path, current_head) {
        worktrees.push(Worktree {
            path,
            head,
            branch: current_branch,
        });
    }

    worktrees
}

/// Get list of worktrees from git worktree list
pub fn get_worktree_list(hub_root: &Path) -> Result<Vec<Worktree>, GitError> {
    let output = run_git_in_dir(hub_root, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_list(&output))
}

/// Get the default branch name from a bare repository.
/// Runs `git symbolic-ref HEAD` to resolve the default branch (e.g. "main").
pub fn get_default_branch(hub_root: &Path) -> Option<String> {
    let output = run_git_in_dir(hub_root, &["symbolic-ref", "HEAD"]).ok()?;
    output.strip_prefix("refs/heads/").map(|s| s.to_string())
}

/// Get the name of the current worktree based on the current directory
/// Returns None if not currently in a worktree (e.g., in the hub root)
pub fn get_current_worktree_name(hub_root: &Path) -> Result<Option<String>, GitError> {
    let current_dir = std::env::current_dir()
        .map_err(|e| GitError::new(format!("Cannot get current dir: {}", e)))?;

    let current_dir = current_dir
        .canonicalize()
        .unwrap_or_else(|_| current_dir.clone());

    let worktrees = get_worktree_list(hub_root)?;

    for wt in worktrees {
        // Skip the bare repo entry
        if wt.head == "(bare)" {
            continue;
        }

        let wt_path = wt.path.canonicalize().unwrap_or_else(|_| wt.path.clone());

        // Check if current dir is the worktree or inside it
        if current_dir.starts_with(&wt_path) {
            if let Some(name) = wt.path.file_name() {
                return Ok(Some(name.to_string_lossy().to_string()));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_list_empty() {
        let output = "";
        let result = parse_worktree_list(output);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_worktree_list_bare_only() {
        let output = "worktree /home/user/project/.bare\nbare\n";
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/home/user/project/.bare"));
        assert_eq!(result[0].head, "(bare)");
        assert!(result[0].branch.is_none());
    }

    #[test]
    fn test_parse_worktree_list_single_worktree() {
        let output =
            "worktree /home/user/project/main\nHEAD abc1234567890def\nbranch refs/heads/main\n";
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/home/user/project/main"));
        assert_eq!(result[0].head, "abc1234567890def");
        assert_eq!(result[0].branch, Some("refs/heads/main".to_string()));
    }

    #[test]
    fn test_parse_worktree_list_multiple_worktrees() {
        let output = "\
worktree /home/user/project/.bare
bare

worktree /home/user/project/main
HEAD abc1234567890def
branch refs/heads/main

worktree /home/user/project/feature
HEAD def4567890abc123
branch refs/heads/feature-branch
";
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 3);

        assert_eq!(result[0].path, PathBuf::from("/home/user/project/.bare"));
        assert_eq!(result[0].head, "(bare)");

        assert_eq!(result[1].path, PathBuf::from("/home/user/project/main"));
        assert_eq!(result[1].head, "abc1234567890def");
        assert_eq!(result[1].branch, Some("refs/heads/main".to_string()));

        assert_eq!(result[2].path, PathBuf::from("/home/user/project/feature"));
        assert_eq!(result[2].head, "def4567890abc123");
        assert_eq!(
            result[2].branch,
            Some("refs/heads/feature-branch".to_string())
        );
    }

    #[test]
    fn test_parse_worktree_list_detached_head() {
        let output = "worktree /home/user/project/detached\nHEAD abc1234567890def\n";
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/home/user/project/detached"));
        assert_eq!(result[0].head, "abc1234567890def");
        assert!(result[0].branch.is_none());
    }

    #[test]
    fn test_git_error_display() {
        let error = GitError::new("test error message");
        assert_eq!(format!("{}", error), "test error message");
    }

    #[test]
    fn test_git_error_new() {
        let error = GitError::new(String::from("owned string"));
        assert_eq!(error.message, "owned string");

        let error = GitError::new("static str");
        assert_eq!(error.message, "static str");
    }

    // --- RepoContext / detect_repo ---

    use std::process::Stdio;
    use tempfile::TempDir;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("failed to spawn git");
        assert!(status.success(), "git {:?} failed", args);
    }

    /// Create a standard repo with one empty commit, returns the temp dir.
    fn init_standard_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        git(tmp.path(), &["init", "-q"]);
        git(
            tmp.path(),
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "init",
            ],
        );
        tmp
    }

    #[test]
    fn test_repo_context_bare_paths() {
        let ctx = RepoContext {
            layout: Layout::Bare {
                hub_root: PathBuf::from("/project"),
            },
        };
        assert_eq!(ctx.anchor_dir(), Path::new("/project"));
        assert_eq!(ctx.state_dir(), PathBuf::from("/project/.wtree"));
        assert_eq!(ctx.worktree_base(), PathBuf::from("/project"));
        assert_eq!(ctx.main_worktree(), None);
        assert!(!ctx.is_standard());
    }

    #[test]
    fn test_repo_context_standard_paths() {
        let ctx = RepoContext {
            layout: Layout::Standard {
                main_worktree: PathBuf::from("/project"),
                common_dir: PathBuf::from("/project/.git"),
            },
        };
        assert_eq!(ctx.anchor_dir(), Path::new("/project"));
        assert_eq!(ctx.state_dir(), PathBuf::from("/project/.git/wtree"));
        assert_eq!(
            ctx.worktree_base(),
            PathBuf::from("/project/.claude/worktrees")
        );
        assert_eq!(ctx.main_worktree(), Some(Path::new("/project")));
        assert!(ctx.is_standard());
    }

    #[test]
    fn test_detect_bare_hub_layout() {
        // Reproduce the smoke `.git -> ./.bare` layout: a `.bare` git dir plus a
        // `.git` file. Detection must classify it as Bare without --show-toplevel.
        let tmp = TempDir::new().unwrap();
        git(tmp.path(), &["init", "--bare", "-q", ".bare"]);
        std::fs::write(tmp.path().join(".git"), "gitdir: ./.bare\n").unwrap();

        let ctx = detect_repo_from(tmp.path()).unwrap();
        match ctx.layout {
            Layout::Bare { hub_root } => {
                assert_eq!(hub_root, tmp.path());
            }
            other => panic!("expected Bare, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_standard_repo() {
        let tmp = init_standard_repo();
        let ctx = detect_repo_from(tmp.path()).unwrap();
        let state_dir = ctx.state_dir();
        match &ctx.layout {
            Layout::Standard {
                main_worktree,
                common_dir,
            } => {
                assert_eq!(
                    main_worktree.canonicalize().unwrap(),
                    tmp.path().canonicalize().unwrap()
                );
                assert_eq!(common_dir.file_name().unwrap(), ".git");
                assert_eq!(state_dir, common_dir.join("wtree"));
            }
            other => panic!("expected Standard, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_standard_from_linked_worktree() {
        // From inside a *linked* worktree, detection must still resolve the
        // primary worktree, not the linked one (adversarial finding #2).
        let tmp = init_standard_repo();
        git(
            tmp.path(),
            &["worktree", "add", "-q", "wt-linked", "-b", "feat"],
        );
        let linked = tmp.path().join("wt-linked");

        let ctx = detect_repo_from(&linked).unwrap();
        match ctx.layout {
            Layout::Standard { main_worktree, .. } => {
                assert_eq!(
                    main_worktree.canonicalize().unwrap(),
                    tmp.path().canonicalize().unwrap(),
                    "main_worktree must be the primary checkout, not the linked one"
                );
            }
            other => panic!("expected Standard, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_outside_repo_errors() {
        let tmp = TempDir::new().unwrap();
        let err = detect_repo_from(tmp.path()).unwrap_err();
        assert!(err.message.contains("Not inside a wtree repository"));
    }
}
