#!/usr/bin/env python3
"""
Automated Removal Issue Creator

This script creates GitHub issues for tracking the removal of deprecated APIs
after their deprecation window expires.

Usage:
    python scripts/create_removal_issue.py --api="TelemetryData::temp_c" --deprecated-in="1.2.0" --remove-in="1.4.0"
    python scripts/create_removal_issue.py --config=removal_config.json
"""

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timedelta
from pathlib import Path
from typing import Dict, List, Optional


class RemovalIssueCreator:
    """Creates GitHub issues for deprecated API removal tracking."""
    
    def __init__(self, workspace_root: Path):
        self.workspace_root = workspace_root
        self.template_path = workspace_root / ".github" / "ISSUE_TEMPLATE" / "remove_deprecated_api.md"
        
    def load_template(self) -> str:
        """Load the issue template."""
        if not self.template_path.exists():
            raise FileNotFoundError(f"Template not found: {self.template_path}")
            
        with open(self.template_path, 'r') as f:
            return f.read()
            
    def generate_search_pattern(self, api_name: str) -> str:
        """Generate regex pattern for searching API usage."""
        # Convert API name to search pattern
        if "::" in api_name:
            # Method or associated function
            parts = api_name.split("::")
            if len(parts) == 2:
                type_name, method_name = parts
                return f"\\b{re.escape(method_name)}\\b|\\b{re.escape(type_name)}::{re.escape(method_name)}\\b"
        elif "." in api_name and not api_name.startswith("."):
            # Field access
            field_name = api_name.split(".")[-1]
            return f"\\.{re.escape(field_name)}\\b"
        else:
            # Simple identifier
            return f"\\b{re.escape(api_name)}\\b"
            
    def estimate_migration_complexity(self, api_name: str, replacement: str) -> str:
        """Estimate migration complexity based on API type and replacement."""
        if "no replacement" in replacement.lower() or "removed" in replacement.lower():
            return "Manual"
        elif "::" in api_name and "::" in replacement:
            # Method to method replacement
            return "Semi-automatic"
        elif api_name.count(".") == replacement.count("."):
            # Field to field replacement
            return "Automatic"
        else:
            return "Semi-automatic"
            
    def assess_risk_level(self, api_name: str, replacement: str) -> str:
        """Assess risk level of the removal."""
        if "no replacement" in replacement.lower():
            return "High"
        elif "rename" in replacement.lower() or api_name.replace("_", "") in replacement.replace("_", ""):
            return "Low"
        else:
            return "Medium"
            
    def determine_scope(self, api_name: str) -> List[str]:
        """Determine the scope of breaking change."""
        scopes = []
        
        # Check if it's a public API
        if any(keyword in api_name.lower() for keyword in ["config", "settings", "telemetry"]):
            scopes.append("Public API")
            
        # Check if it affects plugin ABI
        if any(keyword in api_name.lower() for keyword in ["plugin", "abi", "frame"]):
            scopes.append("Plugin ABI")
            
        # Check if it affects configuration
        if any(keyword in api_name.lower() for keyword in ["config", "settings", "filter"]):
            scopes.append("Configuration")
            
        # Default to internal if no specific scope identified
        if not scopes:
            scopes.append("Internal only")
            
        return scopes
        
    def create_issue_content(self, 
                           api_name: str,
                           deprecated_version: str, 
                           remove_version: str,
                           replacement: str,
                           migration_guide: str = "",
                           category: str = "") -> str:
        """Create issue content from template."""
        
        template = self.load_template()
        
        # Calculate deprecation window
        try:
            dep_parts = deprecated_version.split(".")
            rem_parts = remove_version.split(".")
            if len(dep_parts) >= 2 and len(rem_parts) >= 2:
                dep_minor = int(dep_parts[1])
                rem_minor = int(rem_parts[1])
                window = rem_minor - dep_minor
            else:
                window = "unknown"
        except (ValueError, IndexError):
            window = "unknown"
            
        # Generate search pattern
        search_pattern = self.generate_search_pattern(api_name)
        
        # Assess migration details
        migration_complexity = self.estimate_migration_complexity(api_name, replacement)
        risk_level = self.assess_risk_level(api_name, replacement)
        scopes = self.determine_scope(api_name)
        
        # Generate example code
        before_example, after_example = self.generate_code_examples(api_name, replacement)
        
        # Fill in template placeholders
        content = template.replace("[API_NAME]", api_name)
        content = content.replace("[DEPRECATED_VERSION]", deprecated_version)
        content = content.replace("[REMOVE_VERSION]", remove_version)
        content = content.replace("[X]", str(window))
        content = content.replace("[SEARCH_PATTERN]", search_pattern)
        
        # Add migration guide link if provided
        if migration_guide:
            content = content.replace("[Link to migration documentation]", migration_guide)
        else:
            content = content.replace("[Link to migration documentation]", "docs/MIGRATION_PATTERNS.md")
            
        # Update code examples
        if before_example and after_example:
            # Find and replace the example code blocks
            before_block = re.search(r'### Before \(Deprecated\)\n```rust\n(.*?)\n```', content, re.DOTALL)
            after_block = re.search(r'### After \(New API\)\n```rust\n(.*?)\n```', content, re.DOTALL)
            
            if before_block:
                content = content.replace(before_block.group(1), before_example)
            if after_block:
                content = content.replace(after_block.group(1), after_example)
                
        # Mark appropriate checkboxes based on assessment
        if migration_complexity == "Automatic":
            content = content.replace("- [ ] **Automatic**: Can be migrated with find/replace", 
                                    "- [x] **Automatic**: Can be migrated with find/replace")
        elif migration_complexity == "Semi-automatic":
            content = content.replace("- [ ] **Semi-automatic**: Requires simple code changes",
                                    "- [x] **Semi-automatic**: Requires simple code changes")
        else:
            content = content.replace("- [ ] **Manual**: Requires significant refactoring",
                                    "- [x] **Manual**: Requires significant refactoring")
                                    
        if risk_level == "Low":
            content = content.replace("- [ ] **Low**: Simple rename or signature change",
                                    "- [x] **Low**: Simple rename or signature change")
        elif risk_level == "Medium":
            content = content.replace("- [ ] **Medium**: Logic changes but clear migration path",
                                    "- [x] **Medium**: Logic changes but clear migration path")
        else:
            content = content.replace("- [ ] **High**: Complex changes affecting multiple systems",
                                    "- [x] **High**: Complex changes affecting multiple systems")
                                    
        # Mark scope checkboxes
        for scope in scopes:
            if scope == "Internal only":
                content = content.replace("- [ ] **Internal only**: Only affects internal workspace code",
                                        "- [x] **Internal only**: Only affects internal workspace code")
            elif scope == "Public API":
                content = content.replace("- [ ] **Public API**: Affects external consumers (requires major version bump)",
                                        "- [x] **Public API**: Affects external consumers (requires major version bump)")
            elif scope == "Plugin ABI":
                content = content.replace("- [ ] **Plugin ABI**: Affects plugin compatibility",
                                        "- [x] **Plugin ABI**: Affects plugin compatibility")
            elif scope == "Configuration":
                content = content.replace("- [ ] **Configuration**: Affects config file formats",
                                        "- [x] **Configuration**: Affects config file formats")
        
        return content
        
    def generate_code_examples(self, api_name: str, replacement: str) -> tuple[str, str]:
        """Generate before/after code examples."""
        
        # Simple field access examples
        if "temp_c" in api_name:
            before = "let temp = telemetry.temp_c;"
            after = "let temp = telemetry.temperature_c;"
        elif "wheel_angle_mdeg" in api_name:
            before = "let angle = telemetry.wheel_angle_mdeg;"
            after = "let angle = telemetry.wheel_angle_deg;"
        elif "faults" in api_name and "fault_flags" in replacement:
            before = "let faults = telemetry.faults;"
            after = "let fault_flags = telemetry.fault_flags;"
        elif "sequence" in api_name:
            before = "let seq = telemetry.sequence;"
            after = "// sequence field removed - use timestamp_us for ordering"
            
        # Constructor examples
        elif "create_device_id" in api_name:
            before = 'let device = create_device_id("wheel-1".to_string());'
            after = 'let device = DeviceId::from_str("wheel-1")?;'
        elif "DeviceId::new" in api_name:
            before = 'let device = DeviceId::new("wheel-1".to_string());'
            after = 'let device = DeviceId::from_str("wheel-1")?;'
            
        # Async pattern examples
        elif "BoxFuture" in api_name:
            before = "fn method(&self) -> BoxFuture<'_, Result<T, E>>;"
            after = "#[async_trait]\nfn method(&self) -> Result<T, E>;"
        elif "impl Future" in api_name:
            before = "fn method(&self) -> impl Future<Output = Result<T, E>>;"
            after = "#[async_trait]\nfn method(&self) -> Result<T, E>;"
            
        # API pattern examples
        elif "glob_reexport" in api_name:
            before = "pub use racing_wheel_schemas::*;"
            after = "use racing_wheel_schemas::prelude::*;"
        elif "private_import" in api_name:
            before = "use racing_wheel_service::internal::Helper;"
            after = "use racing_wheel_service::Helper;"
            
        else:
            # Generic example
            before = f"// Example usage of {api_name}"
            after = f"// Use {replacement} instead"
            
        return before, after
        
    def create_github_issue(self, title: str, body: str, labels: List[str]) -> bool:
        """Create GitHub issue using gh CLI."""
        try:
            cmd = ["gh", "issue", "create", "--title", title, "--body", body]
            
            if labels:
                cmd.extend(["--label", ",".join(labels)])
                
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
            print(f"✅ Created issue: {result.stdout.strip()}")
            return True
            
        except subprocess.CalledProcessError as e:
            print(f"❌ Failed to create GitHub issue: {e}")
            print(f"Error output: {e.stderr}")
            return False
        except FileNotFoundError:
            print("❌ GitHub CLI (gh) not found. Please install it or create the issue manually.")
            print("\nIssue content:")
            print("=" * 50)
            print(f"Title: {title}")
            print(f"Labels: {', '.join(labels)}")
            print("\nBody:")
            print(body)
            return False
            
    def create_removal_issue(self,
                           api_name: str,
                           deprecated_version: str,
                           remove_version: str, 
                           replacement: str = "",
                           migration_guide: str = "",
                           category: str = "",
                           create_github: bool = True) -> str:
        """Create a removal issue for a deprecated API."""
        
        # Generate issue content
        content = self.create_issue_content(
            api_name=api_name,
            deprecated_version=deprecated_version,
            remove_version=remove_version,
            replacement=replacement,
            migration_guide=migration_guide,
            category=category
        )
        
        # Generate title
        title = f"Remove deprecated API: {api_name} in v{remove_version}"
        
        # Labels
        labels = ["breaking-change", "deprecation", "technical-debt"]
        if category:
            labels.append(f"category:{category}")
            
        # Create GitHub issue if requested
        if create_github:
            success = self.create_github_issue(title, content, labels)
            if not success:
                # Save to file as fallback
                filename = f"removal_issue_{api_name.replace('::', '_').replace('.', '_')}.md"
                filepath = self.workspace_root / filename
                with open(filepath, 'w') as f:
                    f.write(f"# {title}\n\n{content}")
                print(f"💾 Issue content saved to {filepath}")
        else:
            print(f"Title: {title}")
            print(f"Labels: {', '.join(labels)}")
            print("\nContent:")
            print(content)
            
        return content


def load_config(config_path: Path) -> List[Dict]:
    """Load removal configuration from JSON file."""
    with open(config_path, 'r') as f:
        return json.load(f)


def main():
    parser = argparse.ArgumentParser(description="Create GitHub issues for deprecated API removal")
    parser.add_argument("--api", help="API name (e.g., 'TelemetryData::temp_c')")
    parser.add_argument("--deprecated-in", help="Version when API was deprecated (e.g., '1.2.0')")
    parser.add_argument("--remove-in", help="Version when API should be removed (e.g., '1.4.0')")
    parser.add_argument("--replacement", default="", help="Replacement API or migration instruction")
    parser.add_argument("--migration-guide", default="", help="Link to migration documentation")
    parser.add_argument("--category", default="", help="API category (e.g., 'telemetry_fields')")
    parser.add_argument("--config", type=Path, help="JSON config file with multiple APIs to process")
    parser.add_argument("--dry-run", action="store_true", help="Print issue content without creating")
    parser.add_argument("--workspace", type=Path, default=Path.cwd(), help="Workspace root directory")
    
    args = parser.parse_args()
    
    # Validate workspace
    workspace_root = args.workspace.resolve()
    if not (workspace_root / "Cargo.toml").exists():
        print(f"Error: {workspace_root} does not appear to be a Rust workspace")
        sys.exit(1)
        
    creator = RemovalIssueCreator(workspace_root)
    
    if args.config:
        # Process multiple APIs from config file
        try:
            apis = load_config(args.config)
            for api_config in apis:
                print(f"\nProcessing {api_config['api']}...")
                creator.create_removal_issue(
                    api_name=api_config["api"],
                    deprecated_version=api_config["deprecated_in"],
                    remove_version=api_config["remove_in"],
                    replacement=api_config.get("replacement", ""),
                    migration_guide=api_config.get("migration_guide", ""),
                    category=api_config.get("category", ""),
                    create_github=not args.dry_run
                )
        except (FileNotFoundError, json.JSONDecodeError, KeyError) as e:
            print(f"Error loading config: {e}")
            sys.exit(1)
            
    elif args.api and args.deprecated_in and args.remove_in:
        # Process single API from command line
        creator.create_removal_issue(
            api_name=args.api,
            deprecated_version=args.deprecated_in,
            remove_version=args.remove_in,
            replacement=args.replacement,
            migration_guide=args.migration_guide,
            category=args.category,
            create_github=not args.dry_run
        )
    else:
        parser.print_help()
        print("\nExample usage:")
        print('  python scripts/create_removal_issue.py --api="TelemetryData::temp_c" --deprecated-in="1.2.0" --remove-in="1.4.0" --replacement="temperature_c"')
        sys.exit(1)


if __name__ == "__main__":
    main()