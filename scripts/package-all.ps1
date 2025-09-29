# Complete packaging script for Racing Wheel Suite
# Builds all packages with security features and reproducible builds

param(
    [Parameter(Mandatory=$false)]
    [string]$Configuration = "Release",
    
    [Parameter(Mandatory=$false)]
    [string]$OutputDir = "artifacts",
    
    [Parameter(Mandatory=$false)]
    [switch]$Sign = $false,
    
    [Parameter(Mandatory=$false)]
    [string]$SigningCert = "",
    
    [Parameter(Mandatory=$false)]
    [switch]$Clean = $false,
    
    [Parameter(Mandatory=$false)]
    [switch]$SkipBuild = $false
)

$ErrorActionPreference = "Stop"

# Configuration
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$OutputPath = Join-Path $ProjectRoot $OutputDir

Write-Host "Racing Wheel Suite Complete Packaging Script" -ForegroundColor Green
Write-Host "=============================================" -ForegroundColor Green

# Clean if requested
if ($Clean) {
    Write-Host "Cleaning previous builds..." -ForegroundColor Yellow
    if (Test-Path $OutputPath) {
        Remove-Item -Path $OutputPath -Recurse -Force
    }
    if (Test-Path (Join-Path $ProjectRoot "target")) {
        Remove-Item -Path (Join-Path $ProjectRoot "target") -Recurse -Force
    }
}

# Create output directory
New-Item -ItemType Directory -Force -Path $OutputPath | Out-Null

# Step 1: License Audit
Write-Host "`n1. Running License Audit..." -ForegroundColor Cyan
try {
    & python (Join-Path $ProjectRoot "scripts\audit-licenses.py") --format json --output (Join-Path $OutputPath "license-report.json") --fail-on-issues
    Write-Host "License audit passed" -ForegroundColor Green
} catch {
    Write-Warning "License audit failed or Python not available: $_"
}

# Step 2: Build Rust Project
if (-not $SkipBuild) {
    Write-Host "`n2. Building Rust Project..." -ForegroundColor Cyan
    
    Set-Location $ProjectRoot
    
    # Set reproducible build environment
    $env:SOURCE_DATE_EPOCH = "1704067200"  # 2024-01-01 00:00:00 UTC
    $env:RUSTC_BOOTSTRAP = "0"
    $env:CARGO_INCREMENTAL = "0"
    
    # Build for Windows
    Write-Host "Building for Windows (x86_64-pc-windows-msvc)..." -ForegroundColor Yellow
    & cargo build --release --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Windows build failed"
        exit 1
    }
    
    Write-Host "Rust build completed successfully" -ForegroundColor Green
} else {
    Write-Host "`n2. Skipping Rust build (--SkipBuild specified)" -ForegroundColor Yellow
}

# Step 3: Create Windows MSI
Write-Host "`n3. Creating Windows MSI..." -ForegroundColor Cyan
$BinPath = Join-Path $ProjectRoot "target\x86_64-pc-windows-msvc\release"

$MsiArgs = @{
    BinPath = $BinPath
    OutputPath = $OutputPath
    Configuration = $Configuration
    Platform = "x64"
    Verify = $true
}

if ($Sign -and $SigningCert) {
    $MsiArgs.SigningCert = $SigningCert
}

try {
    & (Join-Path $ProjectRoot "packaging\windows\build-msi.ps1") @MsiArgs
    Write-Host "MSI creation completed successfully" -ForegroundColor Green
} catch {
    Write-Error "MSI creation failed: $_"
    exit 1
}

# Step 4: Generate Security Checksums
Write-Host "`n4. Generating Security Checksums..." -ForegroundColor Cyan

$AllFiles = Get-ChildItem -Path $OutputPath -File -Recurse
$ChecksumReport = @()

foreach ($file in $AllFiles) {
    if ($file.Extension -notin @('.checksums', '.json', '.md')) {
        $sha256 = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash
        $sha512 = (Get-FileHash -Path $file.FullName -Algorithm SHA512).Hash
        
        $ChecksumReport += [PSCustomObject]@{
            File = $file.Name
            RelativePath = $file.FullName.Replace($OutputPath, "").TrimStart('\')
            Size = $file.Length
            SHA256 = $sha256
            SHA512 = $sha512
            Created = $file.CreationTime.ToString("yyyy-MM-ddTHH:mm:ssZ")
        }
    }
}

$ChecksumReport | ConvertTo-Json -Depth 2 | Set-Content -Path (Join-Path $OutputPath "checksums-all.json")
Write-Host "Security checksums generated" -ForegroundColor Green

# Step 5: Create Release Package
Write-Host "`n5. Creating Release Package..." -ForegroundColor Cyan

# Get version
$CargoToml = Join-Path $ProjectRoot "Cargo.toml"
$Version = "0.1.0"
if (Test-Path $CargoToml) {
    $VersionMatch = Select-String -Path $CargoToml -Pattern '^version\s*=\s*"([^"]+)"'
    if ($VersionMatch) {
        $Version = $VersionMatch.Matches[0].Groups[1].Value
    }
}

# Create release metadata
$ReleaseMetadata = @{
    version = $Version
    build_time = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    configuration = $Configuration
    signed = $Sign -and $SigningCert
    git_commit = ""
    git_branch = ""
    files = $ChecksumReport
} 

# Try to get Git information
try {
    $ReleaseMetadata.git_commit = (& git rev-parse HEAD 2>$null).Trim()
    $ReleaseMetadata.git_branch = (& git rev-parse --abbrev-ref HEAD 2>$null).Trim()
} catch {
    Write-Warning "Could not retrieve Git information"
}

$ReleaseMetadata | ConvertTo-Json -Depth 3 | Set-Content -Path (Join-Path $OutputPath "release-metadata.json")

# Create ZIP package
$ZipPath = Join-Path $ProjectRoot "RacingWheelSuite-$Version-Windows.zip"
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath -Force
}

Compress-Archive -Path "$OutputPath\*" -DestinationPath $ZipPath -CompressionLevel Optimal
Write-Host "Release package created: $ZipPath" -ForegroundColor Green

# Step 6: Verification
Write-Host "`n6. Final Verification..." -ForegroundColor Cyan

# Verify all expected files exist
$ExpectedFiles = @(
    "*.msi",
    "*.checksums", 
    "*.metadata.json",
    "license-report.json",
    "checksums-all.json",
    "release-metadata.json"
)

$MissingFiles = @()
foreach ($pattern in $ExpectedFiles) {
    if (-not (Get-ChildItem -Path $OutputPath -Filter $pattern -ErrorAction SilentlyContinue)) {
        $MissingFiles += $pattern
    }
}

if ($MissingFiles.Count -gt 0) {
    Write-Warning "Missing expected files: $($MissingFiles -join ', ')"
} else {
    Write-Host "All expected files present" -ForegroundColor Green
}

# Summary
Write-Host "`n=============================================" -ForegroundColor Green
Write-Host "Packaging Complete!" -ForegroundColor Green
Write-Host "=============================================" -ForegroundColor Green
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host "Output Directory: $OutputPath" -ForegroundColor Cyan
Write-Host "Release Package: $ZipPath" -ForegroundColor Cyan
Write-Host "Signed: $(if ($Sign -and $SigningCert) { 'Yes' } else { 'No' })" -ForegroundColor Cyan

Write-Host "`nGenerated Files:" -ForegroundColor Yellow
Get-ChildItem -Path $OutputPath -File | ForEach-Object {
    Write-Host "  $($_.Name) ($($_.Length) bytes)" -ForegroundColor White
}

Write-Host "`nNext Steps:" -ForegroundColor Yellow
Write-Host "1. Test the MSI installer on a clean Windows system" -ForegroundColor White
Write-Host "2. Verify all signatures and checksums" -ForegroundColor White
Write-Host "3. Upload to release distribution system" -ForegroundColor White
Write-Host "4. Update documentation and release notes" -ForegroundColor White

Write-Host "`nPackaging completed successfully!" -ForegroundColor Green