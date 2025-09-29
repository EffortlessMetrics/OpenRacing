# Shell Completion Scripts

This directory contains shell completion scripts for `wheelctl`.

## Installation

### Bash

```bash
# Generate and install completion script
wheelctl completion bash > ~/.wheelctl-completion.bash
echo 'source ~/.wheelctl-completion.bash' >> ~/.bashrc

# Or use eval (loads on each shell startup)
echo 'eval "$(wheelctl completion bash)"' >> ~/.bashrc
```

### Zsh

```zsh
# Create completions directory if it doesn't exist
mkdir -p ~/.zsh/completions

# Generate completion script
wheelctl completion zsh > ~/.zsh/completions/_wheelctl

# Add to fpath in ~/.zshrc (if not already there)
echo 'fpath=(~/.zsh/completions $fpath)' >> ~/.zshrc
echo 'autoload -U compinit && compinit' >> ~/.zshrc

# Or use eval method
echo 'eval "$(wheelctl completion zsh)"' >> ~/.zshrc
```

### Fish

```fish
# Generate and install completion script
wheelctl completion fish > ~/.config/fish/completions/wheelctl.fish
```

### PowerShell

```powershell
# Add to PowerShell profile
wheelctl completion powershell | Out-String | Invoke-Expression

# Or save to file and dot-source
wheelctl completion powershell > wheelctl-completion.ps1
# Add '. ./wheelctl-completion.ps1' to your profile
```

## Features

The completion scripts provide:

- Command and subcommand completion
- Option and flag completion
- Device ID completion (when service is available)
- Profile path completion
- Game ID completion for supported games
- File path completion for relevant arguments

## Verification

After installation, restart your shell and test completion:

```bash
wheelctl <TAB>
wheelctl device <TAB>
wheelctl profile apply <TAB>
```