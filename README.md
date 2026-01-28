# wtree

A CLI tool for managing git worktrees using a bare repository structure.

Git worktrees let you work on multiple branches simultaneously in separate directories. No stashing, no context switching, and all worktrees share the same git objects.

## Installation

### Cargo

```bash
cargo install wtree
```

### From source

```bash
git clone https://github.com/imqdee/wtree.git
cd wtree
cargo build --release
cp target/release/wt ~/.local/bin/  # or anywhere in your PATH
```

## Shell Setup

Add to your `.bashrc` or `.zshrc`:

```bash
# Bash
eval "$(wt init bash)"

# Zsh
eval "$(wt init zsh)"
```

This wraps the `wt` command so that `switch` (and the `-s` flag) automatically changes your directory to the target worktree.

## Usage

| Command                             | Description                                     |
| ----------------------------------- | ----------------------------------------------- |
| `wt clone <url> [-s]`               | Clone repo as bare with default branch worktree |
| `wt create <name> [-b branch] [-s]` | Create new worktree (alias: `c`)                |
| `wt switch <name> [-e]`             | Switch to worktree (alias: `sw`)                |
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
wt sw feature-auth -e               # switch and copy .env files from origin to destination

# List and remove
wt ls
wt rm feature-auth                  # remove single worktree
wt rm feature-one feature-two       # remove multiple worktrees
```

### Flags

- `-s, --switch`: After clone/create, switch to the new worktree
- `-b, --branch <name>`: Base branch for new worktree (create only)
- `-e, --envs`: Copy `.env*` files (except `.env.example`) when switching

## Hooks

Define pre/post commands for `create`, `switch`, and `remove` operations.

### Configuration

A template `.wtree/hooks.toml` is automatically created when you clone a repository with `wt clone`. Uncomment the hooks you want to enable:

```toml
[create]
pre = ["echo 'Creating worktree...'"]
post = ["cp ../.env .env", "npm install"]

[switch]
pre = []
post = ["npm install"]

[remove]
pre = ["echo 'Cleaning up...'"]
post = []
```

### Environment Variables

Hooks receive context via environment variables:

| Variable           | Description                        | Available in |
| ------------------ | ---------------------------------- | ------------ |
| `WT_COMMAND`       | Command name (create/switch/remove)| All hooks    |
| `WT_WORKTREE_NAME` | Name of the target worktree        | All hooks    |
| `WT_WORKTREE_PATH` | Absolute path to target worktree   | All hooks    |
| `WT_HUB_ROOT`      | Path to hub root (parent of .bare) | All hooks    |
| `WT_BRANCH`        | Branch name (if specified)         | create only  |

### Behavior

- **Pre-hooks** run from the hub root directory. If a pre-hook fails, the command is aborted.
- **Post-hooks** run from the target worktree directory. If a post-hook fails, a warning is logged but the command completes.

## Development

```bash
cargo build           # debug build
cargo build --release # release build
cargo test            # run tests
```

## License

MIT
