# wtree

A CLI tool for managing git worktrees with powerful workflow automation.

Git worktrees let you work on multiple branches simultaneously in separate directories. **wtree** wraps git worktree with a lifecycle hook system that automates your workflow—install dependencies, copy environment files, run setup scripts—all triggered automatically when you create, switch, or remove worktrees.

## Why wtree?

**Workflow Automation via Hooks**

Most worktree tools stop at `git worktree add`. wtree goes further with a two-phase hook system that automates your entire development workflow:

```toml
# .wtree/hooks.toml
[create]
post = [
  "cp \"$WT_HUB_ROOT/main/.env\" \"$WT_WORKTREE_PATH/.env\"",
  "npm install"
]
```

Every time you create a new worktree, your environment is automatically set up. Switch to an existing worktree? Dependencies are synced. No manual steps, no forgotten setup.

## Installation

### Cargo

```bash
cargo install wtree
```

## Shell Setup

Add to your `.bashrc` or `.zshrc`:

```bash
# Bash
eval "$(wt init bash)"

# Zsh
eval "$(wt init zsh)"
```

## Usage

| Command                             | Description                                     |
| ----------------------------------- | ----------------------------------------------- |
| `wt clone <url> [-s]`               | Clone repo as bare with default branch worktree |
| `wt create <name> [-b branch] [-s]` | Create new worktree (alias: `c`)                |
| `wt switch <name>`                  | Switch to worktree (alias: `sw`)                |
| `wt list`                           | List all worktrees (alias: `ls`)                |
| `wt remove <name>...`               | Remove one or more worktrees (alias: `rm`)      |

### Examples

```bash
# Clone a repository
wt clone git@github.com:user/repo.git
wt clone git@github.com:user/repo.git -s  # clone and switch to default branch

# Create worktrees
wt create feature-auth              # from current HEAD
wt create hotfix -b main            # from specific branch
wt create feature-ui -s             # create and switch to the branch

# Switch between worktrees
wt switch main
wt sw feature-auth

# List and remove
wt ls
wt rm feature-auth                  # remove single worktree
wt rm feature-one feature-two       # remove multiple worktrees
```

### Flags

- `-s, --switch`: After clone/create, switch to the new worktree
- `-b, --branch <name>`: Base branch for new worktree (create only)

## Hooks

The hook system is wtree's core feature. Define shell commands that run automatically during worktree lifecycle events.

### Local Hooks

When you clone a repository with `wt clone`, a customisble template `.wtree/hooks.toml` is created:

### Global Default Hooks

Define default hooks that apply to all new repositories:

```bash
mkdir -p ~/.wtree
cat > ~/.wtree/default-hooks.toml << 'EOF'
[create]
pre = []
post = ["cp \"$WT_HUB_ROOT/main/.env\" \"$WT_WORKTREE_PATH/.env\""]

[switch]
pre = []
post = []

[remove]
pre = []
post = []
EOF
```

When you run `wt clone`, the tool will:

1. Check if `~/.wtree/default-hooks.toml` exists
2. If it exists, copy its content as the new repository's `.wtree/hooks.toml`
3. If it doesn't exist, use the built-in template with commented examples

### Environment Variables

Hooks receive full context via environment variables:

| Variable           | Description                         | Available in |
| ------------------ | ----------------------------------- | ------------ |
| `WT_COMMAND`       | Command name (create/switch/remove) | All hooks    |
| `WT_WORKTREE_NAME` | Name of the target worktree         | All hooks    |
| `WT_WORKTREE_PATH` | Absolute path to target worktree    | All hooks    |
| `WT_HUB_ROOT`      | Path to hub root (parent of .bare)  | All hooks    |
| `WT_BRANCH`        | Branch name (if specified)          | create only  |

### Execution Model

| Phase      | Working Directory          | On Failure                        |
| ---------- | -------------------------- | --------------------------------- |
| Pre-hooks  | Hub root (parent of .bare) | Command aborted                   |
| Post-hooks | Target worktree directory  | Warning logged, command completes |

This design lets you:

- Use **pre-hooks** as gates (validate branch names, check prerequisites)
- Use **post-hooks** for setup (install deps, copy files) without blocking on failures

### Real-World Examples

**Node.js project with shared environment:**

```toml
[create]
post = [
  "cp \"$WT_HUB_ROOT/main/.env\" \"$WT_WORKTREE_PATH/.env\"",
  "npm install"
]

[switch]
post = ["npm install"]
```

**Python project with virtual environments:**

```toml
[create]
post = [
  "python -m venv .venv",
  "source .venv/bin/activate && pip install -r requirements.txt"
]

[switch]
post = ["source .venv/bin/activate"]
```

**Validate branch naming convention:**

```toml
[create]
pre = ["[[ \"$WT_WORKTREE_NAME\" =~ ^(feature|fix|hotfix)- ]]"]
post = []
```

## Development

```bash
cargo build           # debug build
cargo build --release # release build
cargo test            # run tests
```

### Install from source

```bash
git clone https://github.com/imqdee/wtree.git
cd wtree
cargo build --release
cp target/release/wt ~/.local/bin/  # or anywhere in your PATH
```

### Development Git Hooks

This project uses [lefthook](https://github.com/evilmartians/lefthook) for git hooks.

```bash
# Install lefthook (macOS)
brew install lefthook

# Install hooks
lefthook install
```

Hooks run automatically on commit (fmt, clippy) and push (test, build).

## License

MIT
