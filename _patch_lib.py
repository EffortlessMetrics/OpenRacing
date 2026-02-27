"""Patch telemetry-adapters lib.rs to add simhub/mudrunner adapters."""
import sys

path = r'H:\Code\Rust\OpenRacing\crates\telemetry-adapters\src\lib.rs'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()

orig_len = len(c)
print(f"Original length: {orig_len}")

changes = 0

# 1. Add pub mod declarations (maintain alphabetical order)
# Add dakar, flatout, mudrunner, simhub modules
old_mods = 'pub mod rfactor2;\npub mod trackmania;'
new_mods = (
    'pub mod mudrunner;\n'
    'pub mod rfactor2;\n'
    'pub mod simhub;\n'
    'pub mod trackmania;'
)
if old_mods in c:
    c = c.replace(old_mods, new_mods, 1)
    changes += 1
    print("Added mudrunner/simhub module declarations")
else:
    print("WARNING: rfactor2/trackmania mod pattern not found")

# Add dakar and flatout if not already there
if 'pub mod dakar;' not in c:
    old_dakar = 'pub mod codemasters_udp;\npub mod dirt4;'
    new_dakar = 'pub mod codemasters_udp;\npub mod dakar;\npub mod dirt4;'
    if old_dakar in c:
        c = c.replace(old_dakar, new_dakar, 1)
        changes += 1
        print("Added dakar module declaration")
    else:
        print("WARNING: dakar insertion point not found")

if 'pub mod flatout;' not in c:
    old_flatout = 'pub mod ets2;\npub mod f1;'
    new_flatout = 'pub mod ets2;\npub mod f1;\npub mod flatout;'
    # Actually flatout comes after f1_25 alphabetically
    old_flatout2 = 'pub mod f1_25;\npub mod forza;'
    new_flatout2 = 'pub mod f1_25;\npub mod flatout;\npub mod forza;'
    if old_flatout2 in c:
        c = c.replace(old_flatout2, new_flatout2, 1)
        changes += 1
        print("Added flatout module declaration")
    else:
        print("WARNING: flatout insertion point not found")

# 2. Add factory functions after new_trackmania_adapter
old_trackmania_fn = (
    'fn new_trackmania_adapter() -> Box<dyn TelemetryAdapter> {\n'
    '    Box::new(TrackmaniaAdapter::new())\n'
    '}'
)
new_fns = old_trackmania_fn + """

fn new_simhub_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(SimHubAdapter::new())
}

fn new_mudrunner_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(mudrunner::MudRunnerAdapter::with_variant(
        mudrunner::MudRunnerVariant::MudRunner,
    ))
}

fn new_snowrunner_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(mudrunner::MudRunnerAdapter::with_variant(
        mudrunner::MudRunnerVariant::SnowRunner,
    ))
}"""

if old_trackmania_fn in c:
    c = c.replace(old_trackmania_fn, new_fns, 1)
    changes += 1
    print("Added simhub/mudrunner/snowrunner factory functions")
else:
    print("WARNING: new_trackmania_adapter fn not found")

# 3. Add registry entries - find the end of the slice
# The slice ends with something like ]\n}\n
# Find the last entry before the closing bracket
import re
# Find the adapter_factories function body
match = re.search(r'pub fn adapter_factories\(\)[^{]*\{[^}]+\}', c, re.DOTALL)
if match:
    body = match.group(0)
    # Find the last entry line
    last_entry = body.rfind('("')
    if last_entry >= 0:
        # Find the end of that line
        end_of_line = body.find('\n', last_entry)
        if end_of_line >= 0:
            # Find the actual position in c
            fn_start = c.find('pub fn adapter_factories()')
            abs_pos = fn_start + end_of_line
            # Insert new entries after this line
            insert_text = (
                '\n        ("simhub", new_simhub_adapter),'
                '\n        ("mudrunner", new_mudrunner_adapter),'
                '\n        ("snowrunner", new_snowrunner_adapter),'
            )
            # Only insert if not already there
            if '"simhub"' not in c[fn_start:fn_start+len(body)+200]:
                c = c[:abs_pos+1] + insert_text + c[abs_pos+1:]
                changes += 1
                print("Added registry entries for simhub/mudrunner/snowrunner")
            else:
                print("Registry entries already present")

# 4. Add pub use exports before pub use trackmania
old_use = 'pub use trackmania::TrackmaniaAdapter;'
new_use = (
    'pub use mudrunner::MudRunnerAdapter;\n'
    'pub use simhub::SimHubAdapter;\n'
    'pub use trackmania::TrackmaniaAdapter;'
)
if old_use in c and 'pub use mudrunner' not in c:
    c = c.replace(old_use, new_use, 1)
    changes += 1
    print("Added pub use exports for mudrunner/simhub")

# Also add pub use for dakar and flatout
if 'pub use dakar' not in c and 'DakarDesertRallyAdapter' in c:
    old_use_n = 'pub use nascar::NascarAdapter;'
    new_use_n = 'pub use dakar::DakarDesertRallyAdapter;\npub use flatout::FlatOutAdapter;\npub use nascar::NascarAdapter;'
    if old_use_n in c:
        c = c.replace(old_use_n, new_use_n, 1)
        changes += 1
        print("Added pub use exports for dakar/flatout")

print(f"\nTotal changes: {changes}")
print(f"New length: {len(c)}")

# Verify
for kw in ['pub mod mudrunner', 'pub mod simhub', 'new_simhub_adapter',
           'new_mudrunner_adapter', '"simhub"', 'pub use mudrunner',
           'pub use simhub']:
    print(f"  {kw}: {'YES' if kw in c else 'NO'}")

with open(path, 'w', encoding='utf-8', newline='\n') as f:
    f.write(c)
print("\nFile written successfully!")
