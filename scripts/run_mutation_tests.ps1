# Run mutation tests for the safety-critical engine code.
#
# Usage:
#   .\scripts\run_mutation_tests.ps1 [-Jobs 4] [-Timeout 60] [-Output C:\tmp\mutants-out]
#
# Prerequisites:
#   cargo install cargo-mutants
#
# Exits with code 1 if any mutants survive (suitable for CI gates).

[CmdletBinding()]
param(
    [int]    $Jobs    = $env:MUTATION_JOBS    ?? 4,
    [int]    $Timeout = $env:MUTATION_TIMEOUT ?? 60,
    [string] $Output  = $env:MUTATION_OUTPUT_DIR ?? "$env:TEMP\mutants-out"
)

$ErrorActionPreference = 'Stop'

Write-Host "=== Mutation Testing: racing-wheel-engine (safety-critical) ===" -ForegroundColor Cyan
Write-Host "  Jobs:    $Jobs"
Write-Host "  Timeout: ${Timeout}s per mutant"
Write-Host "  Output:  $Output"
Write-Host ""

cargo mutants `
    --package racing-wheel-engine `
    --test-timeout $Timeout `
    --jobs $Jobs `
    --output $Output

$ExitCode = $LASTEXITCODE

$Summary = Join-Path $Output "mutants.out\outcomes.json"
if (Test-Path $Summary) {
    Write-Host ""
    Write-Host "=== Summary ===" -ForegroundColor Cyan

    $outcomes = Get-Content $Summary -Raw | ConvertFrom-Json
    $caught   = @($outcomes | Where-Object { $_.summary -eq "caught"   }).Count
    $survived = @($outcomes | Where-Object { $_.summary -eq "survived" }).Count
    $timedout = @($outcomes | Where-Object { $_.summary -eq "timeout"  }).Count

    Write-Host "  Caught:   $caught"
    Write-Host "  Survived: $survived"
    Write-Host "  Timeout:  $timedout"

    if ($survived -gt 0) {
        Write-Host ""
        Write-Host "ERROR: $survived mutant(s) survived — add tests to kill them." -ForegroundColor Red
        exit 1
    }
}

exit $ExitCode
