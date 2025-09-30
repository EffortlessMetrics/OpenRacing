# Compilation Error Baseline and Tracking

This directory contains the compilation error baseline and tracking tools for the racing wheel project.

## Files

- `compile-baseline.json` - Initial baseline with 23 errors across 8 crates
- `compile-summary.json` - Summary statistics for quick reference
- `compile-progress.json` - Latest progress comparison (updated by tracker)

## Error Categories

Based on the baseline analysis, errors fall into these categories:

1. **Build Dependencies (2 errors)**: Missing cmake/NASM for aws-lc-sys
2. **Missing Fields (2 errors)**: Unused fields and functions
3. **Other (19 errors)**: Various compilation issues including:
   - Unsafe function calls requiring unsafe blocks
   - Binding modifier issues
   - Package specification mismatches

## Crate Status

| Crate | Status | Errors | Notes |
|-------|--------|--------|-------|
| racing-wheel-schemas | ✅ | 0 | Compiles successfully |
| racing-wheel-engine | ❌ | 5 | Unsafe function calls, binding issues |
| racing-wheel-service | ❌ | 1 | aws-lc-sys build dependency |
| wheelctl | ❌ | 5 | Unused fields/functions |
| racing-wheel-ui | ❌ | 5 | Same as engine (dependency) |
| racing-wheel-plugins | ❌ | 5 | Same as engine (dependency) |
| racing-wheel-integration-tests | ❌ | 1 | aws-lc-sys + invalid wheelctl dep |
| racing-wheel-compat | ❌ | 1 | Package not found |

## Usage

### Create Initial Baseline
```bash
python scripts/create_error_baseline.py
```

### Track Progress
```bash
python scripts/track_compile_progress.py
```

### Analyze Specific Errors
```bash
# Check individual crate
cargo check -p racing-wheel-schemas

# Check with JSON output for detailed analysis
cargo check -p wheelctl --message-format=json
```

## Key Findings

1. **racing-wheel-schemas** is the only crate that compiles successfully
2. **aws-lc-sys dependency** is blocking 3 crates (service, integration-tests, workspace)
3. **racing-wheel-engine** has unsafe code issues affecting dependent crates
4. **wheelctl** has unused code warnings treated as errors
5. **racing-wheel-compat** package doesn't exist in workspace

## Next Steps

The baseline establishes a quantified starting point with:
- 23 total compilation errors
- 9 warnings
- 7 out of 8 crates failing to compile
- Deterministic and reproducible measurement

This baseline can now be used to track progress as compilation fixes are implemented according to the task plan.