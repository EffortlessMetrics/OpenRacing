import subprocess
import sys
import json
import argparse

def main():
    parser = argparse.ArgumentParser(description="Wrapper for cargo fmt to avoid Windows path length limits")
    parser.add_argument("--check", action="store_true", help="Run in check mode")
    args = parser.parse_args()

    # Get workspace metadata
    print("Fetching cargo metadata...")
    try:
        metadata_output = subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            text=True,
            stderr=subprocess.STDOUT
        )
    except subprocess.CalledProcessError as e:
        print(f"Error fetching metadata: {e.output}")
        sys.exit(1)

    metadata = json.loads(metadata_output)
    packages = metadata.get("packages", [])
    
    # We only care about workspace packages
    workspace_members = metadata.get("workspace_members", [])
    workspace_packages = [p for p in packages if p["id"] in workspace_members]

    print(f"Formatting {len(workspace_packages)} workspace crates individually to prevent Windows path length issues...")
    
    failed_crates = []
    
    for pkg in workspace_packages:
        name = pkg["name"]
        print(f"  Formatting {name}...", end=" ", flush=True)
        
        cmd = ["cargo", "fmt", "-p", name]
        if args.check:
            cmd.extend(["--", "--check"])
            
        try:
            result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            if result.returncode == 0:
                print("OK")
            else:
                print("FAILED")
                print(f"  Output:\n{result.stdout}")
                failed_crates.append(name)
        except Exception as e:
            print(f"ERROR: {e}")
            failed_crates.append(name)
            
    if failed_crates:
        print(f"\nFormatting failed for {len(failed_crates)} crates: {', '.join(failed_crates)}")
        sys.exit(1)
        
    print("\nAll crates formatted successfully!")
    sys.exit(0)

if __name__ == "__main__":
    main()
