# PowerShell script to check protobuf schema compatibility using buf
# This should be run in CI to prevent breaking changes

param(
    [switch]$SkipBreaking = $false
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$SchemasDir = Split-Path -Parent $ScriptDir

Push-Location $SchemasDir

try {
    Write-Host "Checking protobuf schema compatibility..." -ForegroundColor Green

    # Check if buf is installed
    if (-not (Get-Command buf -ErrorAction SilentlyContinue)) {
        Write-Error "Error: buf is not installed. Please install buf CLI tool."
        Write-Host "See: https://docs.buf.build/installation" -ForegroundColor Yellow
        exit 1
    }

    # Lint the protobuf files
    Write-Host "Running buf lint..." -ForegroundColor Blue
    buf lint

    # Check for breaking changes against main branch
    if (-not $SkipBreaking) {
        $mainExists = git rev-parse --verify origin/main 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Host "Checking for breaking changes against origin/main..." -ForegroundColor Blue
            buf breaking --against '.git#branch=origin/main,subdir=crates/schemas'
        } else {
            Write-Warning "origin/main not found, skipping breaking change detection"
        }
    }

    # Verify buf workspace and proto correctness
    Write-Host "Verifying buf workspace..." -ForegroundColor Blue
    buf build

    Write-Host "Schema compatibility check completed successfully!" -ForegroundColor Green
}
finally {
    Pop-Location
}
