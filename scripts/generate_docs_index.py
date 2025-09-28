#!/usr/bin/env python3
"""
Documentation index generator.
Generates an index of all ADRs and other documentation.
"""

import os
import re
from pathlib import Path
from typing import List, Dict, Tuple

def extract_adr_info(adr_path: Path) -> Dict[str, str]:
    """Extract metadata from an ADR file."""
    try:
        content = adr_path.read_text(encoding='utf-8')
    except:
        return {}
    
    lines = content.split('\n')
    
    # Extract title
    title_match = re.match(r'^# (ADR-\d{4}: .+)', lines[0] if lines else '')
    title = title_match.group(1) if title_match else adr_path.stem
    
    # Extract metadata
    metadata = {}
    for line in lines[:20]:  # Check first 20 lines for metadata
        if line.startswith('**Status:**'):
            metadata['status'] = re.search(r'\*\*Status:\*\* (.+)', line).group(1).strip()
        elif line.startswith('**Date:**'):
            metadata['date'] = re.search(r'\*\*Date:\*\* (.+)', line).group(1).strip()
        elif line.startswith('**Authors:**'):
            metadata['authors'] = re.search(r'\*\*Authors:\*\* (.+)', line).group(1).strip()
    
    # Extract first paragraph of context as description
    context_started = False
    description = ""
    for line in lines:
        if line.startswith('## Context'):
            context_started = True
            continue
        elif context_started:
            if line.startswith('##'):  # Next section
                break
            elif line.strip() and not description:
                description = line.strip()
                break
    
    return {
        'title': title,
        'description': description,
        'status': metadata.get('status', 'Unknown'),
        'date': metadata.get('date', 'Unknown'),
        'authors': metadata.get('authors', 'Unknown')
    }

def generate_adr_index(adr_dir: Path) -> str:
    """Generate markdown index of all ADRs."""
    adr_files = []
    for file_path in adr_dir.glob("*.md"):
        if file_path.name not in ["template.md", "README.md"]:
            if re.match(r'\d{4}-.*\.md', file_path.name):
                adr_files.append(file_path)
    
    adr_files.sort()
    
    index_lines = [
        "# Architecture Decision Records Index",
        "",
        f"Total ADRs: {len(adr_files)}",
        "",
        "| ADR | Title | Status | Date | Authors |",
        "|-----|-------|--------|------|---------|"
    ]
    
    for adr_path in adr_files:
        info = extract_adr_info(adr_path)
        adr_num = re.match(r'(\d{4})-', adr_path.name).group(1)
        
        index_lines.append(
            f"| [{adr_num}]({adr_path.name}) | {info.get('title', adr_path.stem)} | "
            f"{info.get('status', 'Unknown')} | {info.get('date', 'Unknown')} | "
            f"{info.get('authors', 'Unknown')} |"
        )
    
    index_lines.extend([
        "",
        "## Status Summary",
        ""
    ])
    
    # Count by status
    status_counts = {}
    for adr_path in adr_files:
        info = extract_adr_info(adr_path)
        status = info.get('status', 'Unknown')
        status_counts[status] = status_counts.get(status, 0) + 1
    
    for status, count in sorted(status_counts.items()):
        index_lines.append(f"- **{status}**: {count}")
    
    index_lines.extend([
        "",
        "## Recent Changes",
        ""
    ])
    
    # Sort by date (newest first)
    dated_adrs = []
    for adr_path in adr_files:
        info = extract_adr_info(adr_path)
        date_str = info.get('date', '1900-01-01')
        try:
            # Simple date parsing for YYYY-MM-DD format
            if re.match(r'\d{4}-\d{2}-\d{2}', date_str):
                dated_adrs.append((date_str, adr_path, info))
        except:
            pass
    
    dated_adrs.sort(reverse=True, key=lambda x: x[0])
    
    for date_str, adr_path, info in dated_adrs[:5]:  # Show 5 most recent
        adr_num = re.match(r'(\d{4})-', adr_path.name).group(1)
        index_lines.append(f"- {date_str}: [{info.get('title', adr_path.stem)}]({adr_path.name})")
    
    return "\n".join(index_lines)

def main():
    adr_dir = Path("docs/adr")
    
    if not adr_dir.exists():
        print("❌ ADR directory not found")
        return
    
    print("📚 Generating documentation index...")
    
    index_content = generate_adr_index(adr_dir)
    
    index_file = adr_dir / "INDEX.md"
    index_file.write_text(index_content, encoding='utf-8')
    
    print(f"✅ Generated ADR index: {index_file}")

if __name__ == '__main__':
    main()