mod commands;
mod git;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wt")]
#[command(about = "A git worktree wrapper for bare repositories")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Clone a repository as a bare repo with worktree support
    Clone {
        /// Repository URL (HTTPS or SSH)
        url: String,
    },
    /// Output shell integration script
    Init {
        /// Shell type (bash or zsh)
        shell: String,
    },
    /// Switch to a worktree (prints path for shell wrapper to cd)
    Switch {
        /// Worktree name
        name: String,
    },
    /// Create a new worktree
    Create {
        /// Worktree name
        name: String,
        /// Branch to checkout (uses current HEAD if not specified)
        #[arg(short, long)]
        branch: Option<String>,
    },
    /// List all worktrees
    List,
    /// Remove a worktree
    Remove {
        /// Worktree name
        name: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Clone { url } => commands::clone::run(&url)?,
        Command::Init { shell } => commands::init::run(&shell)?,
        Command::Switch { name } => commands::switch::run(&name)?,
        Command::Create { name, branch } => commands::create::run(&name, branch.as_deref())?,
        Command::List => commands::list::run()?,
        Command::Remove { name } => commands::remove::run(&name)?,
    }

    Ok(())
}
