# OpenRacing Release Signing Script (Windows)
#
# Signs release artifacts and generates SHA256 checksums
#
# Requirements: 19.5, 19.6
# - Sign all packages with release key
# - Generate SHA256 checksums
#
# Usage:
#   .\sign-release.ps1 -ArtifactsDir <dir> [-SigningCert <path>] [-OutputDir <dir>]

param(
    [Parameter(Mandatory=$true)]
    [string]$ArtifactsDir,
    
    [Parameter(Mandatory=$false)]
    [string]$OutputDir = "",
    
    [Parameter(Mandatory=$false)]
    [string]$SigningCert = "",
    
    [Parameter(Mandatory=$false)]
    [string]$TimestampUrl = "http://timestamp.digicert.com",
    
    [Parameter(Mandatory=$false)]
    [switch]$SkipSigning = $false,
    
    [Parameter(Mandatory=$false)]
    [switch]$UseGpg = $false,
    
    [Parameter(Mandatory=$false)]
    [string]$GpgKeyId = ""
)

$ErrorActionPreference = "Stop"

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "OpenRacing Release Signing (Windows)" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

# Validate artifacts directory
if (-not (Test-Path $ArtifactsDir)) {
    Write-Error "Artifacts directory does not exist: $ArtifactsDir"
    exit 1
}

$ArtifactsDir = Resolve-Path $ArtifactsDir

# Set output directory
if ([string]::IsNullOrEmpty($OutputDir)) {
    $OutputDir = $ArtifactsDir
} else {
    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    $OutputDir = Resolve-Path $OutputDir
}

Write-Host ""
Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Artifacts:    $ArtifactsDir"
Write-Host "  Output:       $OutputDir"
Write-Host "  Skip signing: $SkipSigning"
if ($UseGpg) {
    Write-Host "  Signing:      GPG"
} elseif (-not [string]::IsNullOrEmpty($SigningCert)) {
    Write-Host "  Signing:      Code signing certificate"
}
Write-Host ""

# Find all artifacts
$ArtifactPatterns = @("*.tar.gz", "*.zip", "*.deb", "*.rpm", "*.msi", "*.dmg", "*.exe")
$Artifacts = @()

foreach ($pattern in $ArtifactPatterns) {
    $found = Get-ChildItem -Path $ArtifactsDir -Filter $pattern -File -ErrorAction SilentlyContinue
    if ($found) {
        $Artifacts += $found
    }
}

if ($Artifacts.Count -eq 0) {
    Write-Error "No artifacts found in $ArtifactsDir"
    exit 1
}

Write-Host "Found $($Artifacts.Count) artifact(s) to process:" -ForegroundColor Green
foreach ($artifact in $Artifacts) {
    Write-Host "  - $($artifact.Name)" -ForegroundColor Gray
}
Write-Host ""

# ============================================
# Generate SHA256 Checksums
# ============================================
Write-Host "Generating SHA256 checksums..." -ForegroundColor Yellow

$ChecksumsFile = Join-Path $OutputDir "SHA256SUMS.txt"
$ChecksumsContent = @()

foreach ($artifact in $Artifacts) {
    $hash = Get-FileHash -Path $artifact.FullName -Algorithm SHA256
    $line = "$($hash.Hash.ToLower())  $($artifact.Name)"
    
    # Create individual checksum file
    $individualChecksum = "$($artifact.FullName).sha256"
    $line | Set-Content -Path $individualChecksum -Encoding ASCII
    Write-Host "  Created: $($artifact.Name).sha256" -ForegroundColor Green
    
    $ChecksumsContent += $line
}

# Write combined checksums file
$ChecksumsContent | Set-Content -Path $ChecksumsFile -Encoding ASCII
Write-Host "  Created: SHA256SUMS.txt" -ForegroundColor Green

# ============================================
# Generate SHA512 Checksums
# ============================================
Write-Host "Generating SHA512 checksums..." -ForegroundColor Yellow

$Sha512File = Join-Path $OutputDir "SHA512SUMS.txt"
$Sha512Content = @()

foreach ($artifact in $Artifacts) {
    $hash = Get-FileHash -Path $artifact.FullName -Algorithm SHA512
    $Sha512Content += "$($hash.Hash.ToLower())  $($artifact.Name)"
}

$Sha512Content | Set-Content -Path $Sha512File -Encoding ASCII
Write-Host "  Created: SHA512SUMS.txt" -ForegroundColor Green

# ============================================
# Sign Artifacts
# ============================================
if ($SkipSigning) {
    Write-Host "Skipping artifact signing (-SkipSigning specified)" -ForegroundColor Yellow
}
elseif ($UseGpg) {
    Write-Host "Signing artifacts with GPG..." -ForegroundColor Yellow
    
    $gpgPath = Get-Command "gpg.exe" -ErrorAction SilentlyContinue
    if (-not $gpgPath) {
        Write-Error "GPG not found. Install Gpg4win or use -SkipSigning"
        exit 1
    }
    
    $gpgArgs = @("--armor", "--detach-sign")
    if (-not [string]::IsNullOrEmpty($GpgKeyId)) {
        $gpgArgs += @("--local-user", $GpgKeyId)
    }
    
    foreach ($artifact in $Artifacts) {
        $sigFile = "$($artifact.FullName).asc"
        & gpg.exe @gpgArgs --output $sigFile $artifact.FullName
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  Signed: $($artifact.Name).asc" -ForegroundColor Green
        } else {
            Write-Warning "Failed to sign: $($artifact.Name)"
        }
    }
    
    # Sign checksums file
    & gpg.exe @gpgArgs --output "$ChecksumsFile.asc" $ChecksumsFile
    if ($LASTEXITCODE -eq 0) {
        Write-Host "  Signed: SHA256SUMS.txt.asc" -ForegroundColor Green
    }
}
elseif (-not [string]::IsNullOrEmpty($SigningCert)) {
    Write-Host "Signing artifacts with code signing certificate..." -ForegroundColor Yellow
    
    $signtoolPath = Get-Command "signtool.exe" -ErrorAction SilentlyContinue
    if (-not $signtoolPath) {
        # Try to find signtool in Windows SDK
        $sdkPaths = @(
            "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe",
            "${env:ProgramFiles}\Windows Kits\10\bin\*\x64\signtool.exe"
        )
        
        foreach ($sdkPath in $sdkPaths) {
            $found = Get-ChildItem -Path $sdkPath -ErrorAction SilentlyContinue | Select-Object -Last 1
            if ($found) {
                $signtoolPath = $found.FullName
                break
            }
        }
        
        if (-not $signtoolPath) {
            Write-Error "signtool.exe not found. Install Windows SDK or use -SkipSigning"
            exit 1
        }
    }
    
    # Sign executable artifacts (MSI, EXE)
    $signableExtensions = @(".msi", ".exe")
    
    foreach ($artifact in $Artifacts) {
        if ($signableExtensions -contains $artifact.Extension.ToLower()) {
            $signtoolArgs = @(
                "sign",
                "/f", $SigningCert,
                "/t", $TimestampUrl,
                "/d", "OpenRacing - Racing Wheel Force Feedback Suite",
                "/v",
                $artifact.FullName
            )
            
            & $signtoolPath @signtoolArgs
            
            if ($LASTEXITCODE -eq 0) {
                Write-Host "  Signed: $($artifact.Name)" -ForegroundColor Green
            } else {
                Write-Warning "Failed to sign: $($artifact.Name)"
            }
        }
    }
}
else {
    Write-Host "No signing method specified. Artifacts will not be signed." -ForegroundColor Yellow
    Write-Host "Use -SigningCert, -UseGpg, or -SkipSigning" -ForegroundColor Yellow
}

# ============================================
# Generate Release Manifest
# ============================================
Write-Host "Generating release manifest..." -ForegroundColor Yellow

$ManifestFile = Join-Path $OutputDir "MANIFEST.json"

# Extract version from artifact name
$Version = "unknown"
foreach ($artifact in $Artifacts) {
    if ($artifact.Name -match "openracing-([0-9]+\.[0-9]+\.[0-9]+[^-]*)") {
        $Version = $Matches[1]
        break
    }
}

$ArtifactsList = @()
foreach ($artifact in $Artifacts) {
    $hash = Get-FileHash -Path $artifact.FullName -Algorithm SHA256
    $ArtifactsList += @{
        filename = $artifact.Name
        size_bytes = $artifact.Length
        sha256 = $hash.Hash.ToLower()
    }
}

$IsSigned = (-not $SkipSigning) -and ((-not [string]::IsNullOrEmpty($SigningCert)) -or $UseGpg)

$Manifest = @{
    product = "OpenRacing"
    version = $Version
    release_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    artifacts = $ArtifactsList
    checksums = @{
        sha256 = "SHA256SUMS.txt"
        sha512 = "SHA512SUMS.txt"
    }
    signed = $IsSigned
}

$Manifest | ConvertTo-Json -Depth 3 | Set-Content -Path $ManifestFile -Encoding UTF8
Write-Host "  Created: MANIFEST.json" -ForegroundColor Green

# ============================================
# Summary
# ============================================
Write-Host ""
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "Signing Complete!" -ForegroundColor Green
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Output files in: $OutputDir" -ForegroundColor Yellow
Write-Host ""

Write-Host "Generated files:" -ForegroundColor Yellow
$generatedFiles = Get-ChildItem -Path $OutputDir -Include @("*.sha256", "*.sig*", "*.asc", "SHA256SUMS.txt", "SHA512SUMS.txt", "MANIFEST.json") -File -ErrorAction SilentlyContinue
foreach ($file in $generatedFiles) {
    Write-Host "  $($file.Name)" -ForegroundColor Gray
}

Write-Host ""
Write-Host "Verification:" -ForegroundColor Yellow
Write-Host "  PowerShell: Get-Content SHA256SUMS.txt | ForEach-Object { `$parts = `$_ -split '  '; (Get-FileHash `$parts[1] -Algorithm SHA256).Hash.ToLower() -eq `$parts[0] }"
