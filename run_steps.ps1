#!/usr/bin/env pwsh
Set-Location 'H:\Code\Rust\OpenRacing'

Write-Host "=== STEP 1: Run python scripts/sync_yaml.py --fix ===" -ForegroundColor Cyan
python scripts/sync_yaml.py --fix
Write-Host ""

Write-Host "=== STEP 2: Run python scripts/sync_yaml.py --check ===" -ForegroundColor Cyan
python scripts/sync_yaml.py --check
Write-Host ""

Write-Host "=== STEP 3: Run cargo check and show last 20 lines ===" -ForegroundColor Cyan
cargo check -p racing-wheel-telemetry-adapters 2>&1 | tail -20
Write-Host ""

Write-Host "=== STEP 4: Git add and status ===" -ForegroundColor Cyan
git add -A
git status --short
Write-Host ""

Write-Host "=== STEP 5: Git commit ===" -ForegroundColor Cyan
git commit -m "feat: add f1_native factory, add motogp/ride5 to YAML game support matrix

- Add new_f1_native_adapter() factory function and register in adapter_factories()
- Add motogp (port 5556) and ride5 (port 5558) entries to game_support_matrix.yaml
- Sync both game_support_matrix.yaml copies with sync_yaml.py
- All 43 game adapters now have entries in both lib.rs registry and YAML matrix

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
Write-Host ""

Write-Host "=== STEP 6: Git push ===" -ForegroundColor Cyan
git push origin feat/r5-test-coverage-and-integration
