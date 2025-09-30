# Check for forbidden glob re-exports in the codebase
# This script searches for `pub use *::*;` patterns which are forbidden
# in our codebase to maintain clear API boundaries.

param(
    [switch]$Strict = $false
)

$ErrorActionPreference = "Stop"

function Check-GlobReexports {
    param([string]$RootDir)
    
    $violations = @()
    $cratesDir = Join-Path $RootDir "crates"
    
    if (-not (Test-Path $cratesDir)) {
        Write-Warning "Warning: $cratesDir does not exist"
        return $violations
    }
    
    $rustFiles = Get-ChildItem -Path $cratesDir -Recurse -Filter "*.rs"
    
    foreach ($file in $rustFiles) {
        try {
            $lineNum = 0
            Get-Content $file.FullName | ForEach-Object {
                $lineNum++
                if ($_ -match 'pub\s+use\s+.*::\*\s*;') {
                    $violations += @{
                        File = $file.FullName
                        Line = $lineNum
                        Content = $_.Trim()
                    }
                }
            }
        }
        catch {
            Write-Warning "Warning: Could not read $($file.FullName): $_"
        }
    }
    
    return $violations
}

# Main execution
$rootDir = Split-Path -Parent $PSScriptRoot
$violations = Check-GlobReexports -RootDir $rootDir

if ($violations.Count -gt 0) {
    if ($Strict) {
        Write-Host "❌ Found forbidden glob re-exports:" -ForegroundColor Red
        foreach ($violation in $violations) {
            Write-Host "  $($violation.File):$($violation.Line): $($violation.Content)" -ForegroundColor Red
        }
        Write-Host ""
        Write-Host "Glob re-exports (pub use *::*;) are forbidden." -ForegroundColor Red
        Write-Host "Use explicit re-exports or prelude modules instead." -ForegroundColor Red
        exit 1
    } else {
        Write-Host "⚠️  Found glob re-exports (will be addressed in future tasks):" -ForegroundColor Yellow
        foreach ($violation in $violations) {
            Write-Host "  $($violation.File):$($violation.Line): $($violation.Content)" -ForegroundColor Yellow
        }
        Write-Host ""
        Write-Host "Total: $($violations.Count) glob re-exports found" -ForegroundColor Yellow
        Write-Host "Note: These will be refactored to use explicit re-exports or prelude modules" -ForegroundColor Yellow
        exit 0
    }
} else {
    Write-Host "✅ No glob re-exports found" -ForegroundColor Green
    exit 0
}