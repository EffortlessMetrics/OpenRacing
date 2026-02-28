import os

BASE = r"H:\Code\Rust\OpenRacing\crates"

def read(path):
    with open(path, "r", encoding="utf-8") as f:
        return f.read()

def write(path, content):
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)

def insert_before_last_brace(path, new_tests, guard):
    content = read(path)
    if guard in content:
        print("  SKIP: " + os.path.basename(path))
        return
    idx = content.rfind("}")
    new_content = content[:idx] + new_tests + "\n}"
    write(path, new_content)
    after = read(path)
    print("  OK: " + os.path.basename(path) + " (" + str(len(content.splitlines())) + " -> " + str(len(after.splitlines())) + " lines)")

def append_if_absent(path, new_block, guard):
    content = read(path)
    if guard in content:
        print("  SKIP: " + os.path.basename(path))
        return
    new_content = content.rstrip("\n") + "\n" + new_block
    write(path, new_content)
    after = read(path)
    print("  OK: " + os.path.basename(path) + " (" + str(len(content.splitlines())) + " -> " + str(len(after.splitlines())) + " lines)")

# 1. telemetry-contracts/Cargo.toml
append_if_absent(
    os.path.join(BASE, "telemetry-contracts", "Cargo.toml"),
    "[dev-dependencies]\nserde_json = { workspace = true }\n",
    "[dev-dependencies]"
)
