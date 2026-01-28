# scripts/ci_wsl.ps1 - WSL wrapper for Linux CI runner
# Usage: .\scripts\ci_wsl.ps1 --mode fast
#        .\scripts\ci_wsl.ps1 -Distro Ubuntu-22.04 --allow-dirty
#        .\scripts\ci_wsl.ps1 -- --mode fast  (-- is optional)

[CmdletBinding(PositionalBinding=$false)]
param(
    [Parameter()]
    [string]$Distro = $env:OPENRACING_WSL_DISTRO,
    [switch]$NoNix,
    [switch]$DryRun,
    [Parameter(ValueFromRemainingArguments=$true)]
    [string[]]$PassThrough
)

$ErrorActionPreference = "Stop"

function Escape-BashSingleQuoted([string]$s) {
    # Bash: 'foo' -> 'foo'"'"'bar' pattern for embedded single quotes
    return $s -replace "'", "'`"'`"'"
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

# Parse passthrough args: skip any leading `--` if present
$passThroughArgs = @()
$skipNext = $false
foreach ($a in $PassThrough) {
    if ($a -eq "--" -and $passThroughArgs.Count -eq 0) {
        # Skip leading delimiter
        continue
    }
    $passThroughArgs += $a
}

# Check WSL availability
$wslCommand = Get-Command wsl.exe -ErrorAction SilentlyContinue
if (-not $wslCommand) {
    Write-Error "WSL is not available. Install WSL or run scripts/ci_nix.sh on Linux."
    exit 1
}

$wslArgs = @()
if ($Distro) {
    $wslArgs += @("-d", $Distro)
}

# Check Nix availability in WSL
if (-not $NoNix) {
    & wsl.exe @wslArgs -- nix --version 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Nix is not available in the selected WSL distro. Install nix, or pass -NoNix."
        exit 1
    }
}

# Map Windows path -> WSL path. Use forward slashes to avoid bash escaping issues.
$repoRootWin = $repoRoot.Path
$repoRootForward = $repoRootWin -replace "\\", "/"
# Build wslpath args: -d distro must come before -e for exec mode
$wslPathArgs = @()
if ($Distro) {
    $wslPathArgs += @("-d", $Distro)
}
$wslPathArgs += @("-e", "wslpath", "-a", "-u", $repoRootForward)
try {
    $wslPath = & wsl.exe @wslPathArgs
    $wslExitCode = $LASTEXITCODE
} catch {
    Write-Error "Failed to run wslpath: $_"
    exit 1
}
if ($wslExitCode -ne 0 -or [string]::IsNullOrWhiteSpace($wslPath)) {
    Write-Error "Failed to map repo path into WSL (wslpath failed). RepoRoot=$repoRootWin, Exit=$wslExitCode, Path='$wslPath'"
    exit 1
}

$wslPath = $wslPath.Trim()

# Build bash command with properly escaped arguments
$ciArgs = @()
foreach ($a in $passThroughArgs) {
    $ciArgs += "'" + (Escape-BashSingleQuoted $a) + "'"
}
$ciArgsStr = $ciArgs -join " "

# Build the command string
$wslPathEscaped = Escape-BashSingleQuoted $wslPath
$scriptPath = "./scripts/ci_nix.sh"
$nixPrefix = if ($NoNix) { "" } else { "nix develop --command " }

$cmd = @"
set -euo pipefail
cd '$wslPathEscaped' 2>/dev/null || cd '$wslPath'
$nixPrefix bash $scriptPath $ciArgsStr
"@

if ($DryRun) {
    Write-Output "wsl.exe $($wslArgs -join ' ') -- bash -lc `"$cmd`""
    exit 0
}

# Run inside WSL login shell
& wsl.exe @wslArgs -- bash -lc $cmd
exit $LASTEXITCODE
