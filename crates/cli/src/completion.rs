//! Shell completion generation for wheelctl

use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;

use crate::Cli;

/// Generate shell completion script
pub fn generate_completion(shell: Shell) {
    let mut cmd = Cli::command();
    let bin_name = "wheelctl";
    
    generate(shell, &mut cmd, bin_name, &mut io::stdout());
}

/// Print installation instructions for completions
#[allow(dead_code)]
pub fn print_completion_instructions(shell: Shell) {
    match shell {
        Shell::Bash => {
            println!("# Add this to your ~/.bashrc:");
            println!("eval \"$(wheelctl completion bash)\"");
            println!();
            println!("# Or save to a file and source it:");
            println!("wheelctl completion bash > ~/.wheelctl-completion.bash");
            println!("echo 'source ~/.wheelctl-completion.bash' >> ~/.bashrc");
        }
        Shell::Zsh => {
            println!("# Add this to your ~/.zshrc:");
            println!("eval \"$(wheelctl completion zsh)\"");
            println!();
            println!("# Or save to a file in your fpath:");
            println!("wheelctl completion zsh > ~/.zsh/completions/_wheelctl");
            println!("# Make sure ~/.zsh/completions is in your fpath");
        }
        Shell::Fish => {
            println!("# Save completion to fish completions directory:");
            println!("wheelctl completion fish > ~/.config/fish/completions/wheelctl.fish");
        }
        Shell::PowerShell => {
            println!("# Add this to your PowerShell profile:");
            println!("Invoke-Expression (& wheelctl completion powershell | Out-String)");
            println!();
            println!("# Or save to a file and dot-source it:");
            println!("wheelctl completion powershell > wheelctl-completion.ps1");
            println!(". ./wheelctl-completion.ps1");
        }
        _ => {
            println!("Completion generated for {:?}", shell);
            println!("Please refer to your shell's documentation for installation instructions.");
        }
    }
}
