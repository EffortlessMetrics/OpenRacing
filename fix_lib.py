#!/usr/bin/env python3
"""Fix telemetry-adapters lib.rs: add motogp and ride5 entries."""

filepath = r'H:\Code\Rust\OpenRacing\crates\telemetry-adapters\src\lib.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

print(f"Original length: {len(content)}")
print(f"motogp count before: {content.count('motogp')}")

# 1. Add pub mod motogp and ride5 after wtcr
if 'pub mod motogp;' not in content:
    content = content.replace('pub mod wtcr;\n', 'pub mod wtcr;\npub mod motogp;\npub mod ride5;\n', 1)
    print("Added pub mod motogp/ride5")
else:
    print("pub mod motogp already present")

# 2. Add factory functions before the registry doc comment
factory_fns = (
    'fn new_motogp_adapter() -> Box<dyn TelemetryAdapter> {\n'
    '    Box::new(motogp::MotoGPAdapter::new())\n'
    '}\n'
    '\n'
    'fn new_ride5_adapter() -> Box<dyn TelemetryAdapter> {\n'
    '    Box::new(ride5::Ride5Adapter::new())\n'
    '}\n'
    '\n'
)
marker = '/// Returns the canonical adapter factory registry'
if 'new_motogp_adapter' not in content:
    content = content.replace(marker, factory_fns + marker, 1)
    print("Added factory functions")
else:
    print("Factory functions already present")

# 3. Add registry entries after flatout
flatout_entry = '("flatout", new_flatout_adapter),'
motogp_entry = '        ("motogp", new_motogp_adapter),'
ride5_entry = '        ("ride5", new_ride5_adapter),'
if '("motogp"' not in content:
    content = content.replace(
        flatout_entry + '\n    ]',
        flatout_entry + '\n' + motogp_entry + '\n' + ride5_entry + '\n    ]',
        1
    )
    print("Added registry entries")
else:
    print("Registry entries already present")

# 4. Add re-exports after wtcr
wtcr_reexport = 'pub use wtcr::WtcrAdapter;'
if 'pub use motogp::MotoGPAdapter;' not in content:
    content = content.replace(
        wtcr_reexport + '\n',
        wtcr_reexport + '\npub use motogp::MotoGPAdapter;\npub use ride5::Ride5Adapter;\n',
        1
    )
    print("Added re-exports")
else:
    print("Re-exports already present")

with open(filepath, 'w', encoding='utf-8') as f:
    f.write(content)

print(f"Final length: {len(content)}")
print(f"motogp count after: {content.count('motogp')}")
print("Done!")
