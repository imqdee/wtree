mod commands;
mod git;
mod hooks;
mod state;

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
    /// Switch to a worktree
    #[command(visible_alias = "sw")]
    Switch {
        /// Worktree name
        name: String,
    },
    /// Create a new worktree
    #[command(visible_alias = "c")]
    Create {
        /// Worktree name
        name: String,
        /// Check out an existing branch in the new worktree
        #[arg(long)]
        checkout: Option<String>,
        /// Create new worktree branching from another worktree's current commit
        #[arg(long, conflicts_with = "checkout")]
        base: Option<String>,
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
        Command::Switch { name } => commands::switch::run(&name)?,
        Command::Create {
            name,
            checkout,
            base,
            switch,
        } => commands::create::run(&name, checkout.as_deref(), base.as_deref(), switch)?,
        Command::List => commands::list::run()?,
        Command::Remove { names } => commands::remove::run(&names)?,
    }

    Ok(())
}
