[CmdletBinding()]
param(
    [string]$Distro = $env:OPENRACING_WSL_DISTRO,
    [switch]$NoNix,
    [switch]$DryRun
)

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$passThroughArgs = $args
if ($PSBoundParameters.ContainsKey("Distro") -and $Distro -like "--*") {
    $passThroughArgs = @($Distro) + $args
    $Distro = $null
}
$wslCommand = Get-Command wsl.exe -ErrorAction SilentlyContinue
if (-not $wslCommand) {
    Write-Error "WSL is not available. Install WSL or run scripts/ci_nix.sh on Linux."
    exit 1
}

$wslArgs = @()
if ($Distro) {
    $wslArgs += @("-d", $Distro)
}

if (-not $NoNix) {
    & wsl.exe @wslArgs -- nix --version | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Nix is not available in the selected WSL distro. Install nix, or pass -NoNix."
        exit 1
    }
}

$repoRootEscaped = $repoRoot.Path -replace "\\", "\\\\"
$wslPath = & wsl.exe @wslArgs -- wslpath -a -u $repoRootEscaped
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($wslPath)) {
    Write-Error "Failed to map repo path into WSL via wslpath."
    exit 1
}

$wslPath = $wslPath.Trim()

function Escape-BashSingleQuoted([string]$Value) {
    return $Value -replace "'", "'\\''"
}

$wslPathEscaped = Escape-BashSingleQuoted $wslPath
$argList = @()
foreach ($arg in $passThroughArgs) {
    $argList += "'" + (Escape-BashSingleQuoted $arg) + "'"
}
$argString = $argList -join " "

$command = "cd '$wslPathEscaped' && "
if (-not $NoNix) {
    $command += "nix develop -c "
}
$command += "bash scripts/ci_nix.sh"
if ($argString) {
    $command += " $argString"
}

if ($DryRun) {
    Write-Output "wsl.exe $($wslArgs -join ' ') -- bash -lc \"$command\""
    exit 0
}

& wsl.exe @wslArgs -- bash -lc $command
exit $LASTEXITCODE
