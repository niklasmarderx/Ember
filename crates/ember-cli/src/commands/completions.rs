//! Shell completion generation for Ember CLI.
//!
//! Generates shell completions for bash, zsh, fish, and PowerShell.

use anyhow::Result;
use clap::{Command, CommandFactory};
use clap_complete::{generate, Shell};
use std::io;

/// Arguments for the completions command.
#[derive(clap::Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Generate shell completions for the given shell.
pub fn generate_completions<C: CommandFactory>(shell: Shell) -> Result<()> {
    let mut cmd = C::command();
    generate(shell, &mut cmd, "ember", &mut io::stdout());
    Ok(())
}

/// Print installation instructions for the generated completions.
pub fn print_installation_instructions(shell: Shell) {
    eprintln!();
    match shell {
        Shell::Bash => {
            eprintln!("# Bash installation instructions:");
            eprintln!("# Add the following to your ~/.bashrc or ~/.bash_profile:");
            eprintln!();
            eprintln!(
                "# Option 1: Install system-wide (requires sudo):"
            );
            eprintln!("# sudo ember completions bash > /etc/bash_completion.d/ember");
            eprintln!();
            eprintln!("# Option 2: Install for current user:");
            eprintln!("# mkdir -p ~/.local/share/bash-completion/completions");
            eprintln!(
                "# ember completions bash > ~/.local/share/bash-completion/completions/ember"
            );
            eprintln!();
            eprintln!("# Then restart your shell or run: source ~/.bashrc");
        }
        Shell::Zsh => {
            eprintln!("# Zsh installation instructions:");
            eprintln!();
            eprintln!("# Option 1: Using Oh My Zsh:");
            eprintln!("# ember completions zsh > ~/.oh-my-zsh/completions/_ember");
            eprintln!();
            eprintln!("# Option 2: Manual installation:");
            eprintln!("# mkdir -p ~/.zfunc");
            eprintln!("# ember completions zsh > ~/.zfunc/_ember");
            eprintln!("# Then add to ~/.zshrc before compinit:");
            eprintln!("#   fpath+=~/.zfunc");
            eprintln!();
            eprintln!("# Then restart your shell or run: compinit");
        }
        Shell::Fish => {
            eprintln!("# Fish installation instructions:");
            eprintln!();
            eprintln!(
                "# ember completions fish > ~/.config/fish/completions/ember.fish"
            );
            eprintln!();
            eprintln!("# Completions will be loaded automatically on next shell start.");
        }
        Shell::PowerShell => {
            eprintln!("# PowerShell installation instructions:");
            eprintln!();
            eprintln!("# Add to your PowerShell profile (run: notepad $PROFILE):");
            eprintln!("# Invoke-Expression (& ember completions powershell | Out-String)");
            eprintln!();
            eprintln!("# Or save to a file and dot-source it:");
            eprintln!("# ember completions powershell > ~\\Documents\\WindowsPowerShell\\ember_completions.ps1");
            eprintln!("# Then add to profile: . ~\\Documents\\WindowsPowerShell\\ember_completions.ps1");
        }
        Shell::Elvish => {
            eprintln!("# Elvish installation instructions:");
            eprintln!();
            eprintln!("# ember completions elvish >> ~/.elvish/rc.elv");
            eprintln!();
            eprintln!("# Completions will be loaded on next shell start.");
        }
        _ => {
            eprintln!("# See your shell's documentation for completion installation.");
        }
    }
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_variants() {
        // Test that all shell variants are valid
        let shells = [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Elvish,
        ];

        for shell in shells {
            // Just verify the shell enum values exist
            assert!(format!("{:?}", shell).len() > 0);
        }
    }
}