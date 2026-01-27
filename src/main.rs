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
        /// Switch to the default branch worktree after cloning
        #[arg(short, long)]
        switch: bool,
    },
    /// Output shell integration script
    Init {
        /// Shell type (bash or zsh)
        shell: String,
    },
    /// Switch to a worktree (prints path for shell wrapper to cd)
    #[command(visible_alias = "sw")]
    Switch {
        /// Worktree name
        name: String,
        /// Copy .env* files (except .env.example) from current worktree to destination
        #[arg(short = 'e', long)]
        envs: bool,
    },
    /// Create a new worktree
    #[command(visible_alias = "c")]
    Create {
        /// Worktree name
        name: String,
        /// Branch to checkout (uses current HEAD if not specified)
        #[arg(short = 'b', long)]
        branch: Option<String>,
        /// Switch to the worktree after creating
        #[arg(short, long)]
        switch: bool,
    },
    /// List all worktrees
    #[command(visible_alias = "ls")]
    List,
    /// Remove worktrees
    #[command(visible_alias = "rm")]
    Remove {
        /// Worktree names to remove
        names: Vec<String>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Clone { url, switch } => commands::clone::run(&url, switch)?,
        Command::Init { shell } => commands::init::run(&shell)?,
        Command::Switch { name, envs } => commands::switch::run(&name, envs)?,
        Command::Create {
            name,
            branch,
            switch,
        } => commands::create::run(&name, branch.as_deref(), switch)?,
        Command::List => commands::list::run()?,
        Command::Remove { names } => commands::remove::run(&names)?,
    }

    Ok(())
}
