#!/usr/bin/env python3
"""
License audit script for Racing Wheel Suite
Analyzes dependencies and generates license compliance reports
"""

import json
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Set, Tuple
import argparse
from datetime import datetime

# Allowed licenses (SPDX identifiers)
ALLOWED_LICENSES = {
    'MIT',
    'Apache-2.0',
    'BSD-2-Clause',
    'BSD-3-Clause',
    'ISC',
    'Unicode-DFS-2016',
    'Unlicense',
    'CC0-1.0'
}

# Licenses that require legal review
REVIEW_REQUIRED = {
    'GPL-2.0',
    'GPL-3.0',
    'LGPL-2.1',
    'LGPL-3.0',
    'AGPL-3.0',
    'MPL-2.0',
    'EPL-1.0',
    'EPL-2.0'
}

# Licenses that are not allowed
FORBIDDEN_LICENSES = {
    'GPL-2.0-only',
    'GPL-3.0-only',
    'AGPL-3.0-only'
}

class LicenseAuditor:
    def __init__(self, project_root: Path):
        self.project_root = project_root
        self.dependencies = {}
        self.license_issues = []
        
    def run_cargo_metadata(self) -> Dict:
        """Run cargo metadata to get dependency information"""
        try:
            result = subprocess.run(
                ['cargo', 'metadata', '--format-version', '1'],
                cwd=self.project_root,
                capture_output=True,
                text=True,
                check=True
            )
            return json.loads(result.stdout)
        except subprocess.CalledProcessError as e:
            print(f"Error running cargo metadata: {e}")
            sys.exit(1)
    
    def extract_dependencies(self, metadata: Dict) -> Dict[str, Dict]:
        """Extract dependency information from cargo metadata"""
        dependencies = {}
        
        for package in metadata['packages']:
            # Skip workspace packages
            if package['source'] is None:
                continue
                
            name = package['name']
            version = package['version']
            license_field = package.get('license', 'Unknown')
            license_file = package.get('license_file')
            repository = package.get('repository', '')
            authors = package.get('authors', [])
            
            dependencies[f"{name}-{version}"] = {
                'name': name,
                'version': version,
                'license': license_field,
                'license_file': license_file,
                'repository': repository,
                'authors': authors,
                'source': package.get('source', '')
            }
        
        return dependencies
    
    def parse_license_expression(self, license_expr: str) -> Set[str]:
        """Parse SPDX license expression and return individual licenses"""
        if not license_expr or license_expr == 'Unknown':
            return set()
        
        # Simple parsing - in production, use a proper SPDX parser
        # Handle common operators: AND, OR, WITH
        licenses = set()
        
        # Split on common operators and clean up
        parts = license_expr.replace(' OR ', '|').replace(' AND ', '|').replace(' WITH ', '|')
        for part in parts.split('|'):
            license_id = part.strip().replace('(', '').replace(')', '')
            if license_id:
                licenses.add(license_id)
        
        return licenses
    
    def check_license_compliance(self, dependency: Dict) -> List[str]:
        """Check if a dependency's license is compliant"""
        issues = []
        license_expr = dependency['license']
        licenses = self.parse_license_expression(license_expr)
        
        if not licenses:
            issues.append(f"No license information for {dependency['name']}")
            return issues
        
        for license_id in licenses:
            if license_id in FORBIDDEN_LICENSES:
                issues.append(f"Forbidden license {license_id} in {dependency['name']}")
            elif license_id in REVIEW_REQUIRED:
                issues.append(f"License {license_id} requires review in {dependency['name']}")
            elif license_id not in ALLOWED_LICENSES:
                issues.append(f"Unknown/unreviewed license {license_id} in {dependency['name']}")
        
        return issues
    
    def audit_licenses(self) -> Tuple[List[str], Dict]:
        """Perform license audit and return issues and summary"""
        print("Running license audit...")
        
        metadata = self.run_cargo_metadata()
        self.dependencies = self.extract_dependencies(metadata)
        
        all_issues = []
        license_summary = {}
        
        for dep_key, dependency in self.dependencies.items():
            issues = self.check_license_compliance(dependency)
            all_issues.extend(issues)
            
            # Build license summary
            licenses = self.parse_license_expression(dependency['license'])
            for license_id in licenses:
                if license_id not in license_summary:
                    license_summary[license_id] = []
                license_summary[license_id].append(dependency['name'])
        
        return all_issues, license_summary
    
    def generate_report(self, output_format: str = 'json', output_file: str = None):
        """Generate license compliance report"""
        issues, license_summary = self.audit_licenses()
        
        report_data = {
            'timestamp': datetime.utcnow().isoformat() + 'Z',
            'project': 'racing-wheel-suite',
            'total_dependencies': len(self.dependencies),
            'issues_count': len(issues),
            'issues': issues,
            'license_summary': license_summary,
            'dependencies': self.dependencies
        }
        
        if output_format == 'json':
            output = json.dumps(report_data, indent=2)
        elif output_format == 'csv':
            output = self._generate_csv_report(report_data)
        elif output_format == 'html':
            output = self._generate_html_report(report_data)
        else:
            raise ValueError(f"Unsupported output format: {output_format}")
        
        if output_file:
            with open(output_file, 'w') as f:
                f.write(output)
            print(f"Report written to {output_file}")
        else:
            print(output)
        
        return len(issues) == 0
    
    def _generate_csv_report(self, report_data: Dict) -> str:
        """Generate CSV format report"""
        output = []
        output.append("Name,Version,License,Repository,Issues")
        
        for dep_key, dep in report_data['dependencies'].items():
            issues = [issue for issue in report_data['issues'] if dep['name'] in issue]
            issues_str = '; '.join(issues) if issues else 'None'
            
            output.append(f"{dep['name']},{dep['version']},{dep['license']},{dep['repository']},{issues_str}")
        
        return '\n'.join(output)
    
    def _generate_html_report(self, report_data: Dict) -> str:
        """Generate HTML format report"""
        html = f"""<!DOCTYPE html>
<html>
<head>
    <title>License Audit Report - Racing Wheel Suite</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        .header {{ background-color: #f0f0f0; padding: 10px; border-radius: 5px; }}
        .summary {{ margin: 20px 0; }}
        .issues {{ background-color: #ffe6e6; padding: 10px; border-radius: 5px; margin: 10px 0; }}
        .dependencies {{ margin: 20px 0; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #f2f2f2; }}
        .issue {{ color: red; }}
        .ok {{ color: green; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>License Audit Report</h1>
        <p>Project: Racing Wheel Suite</p>
        <p>Generated: {report_data['timestamp']}</p>
    </div>
    
    <div class="summary">
        <h2>Summary</h2>
        <p>Total Dependencies: {report_data['total_dependencies']}</p>
        <p>License Issues: {report_data['issues_count']}</p>
    </div>
    
    <div class="issues">
        <h2>Issues</h2>"""
        
        if not report_data['issues']:
            html += '<p class="ok">No license issues found!</p>'
        else:
            html += '<ul>'
            for issue in report_data['issues']:
                html += f'<li class="issue">{issue}</li>'
            html += '</ul>'
        
        html += """
    </div>
    
    <div class="dependencies">
        <h2>Dependencies</h2>
        <table>
            <tr>
                <th>Name</th>
                <th>Version</th>
                <th>License</th>
                <th>Repository</th>
            </tr>"""
        
        for dep in report_data['dependencies'].values():
            html += f"""
            <tr>
                <td>{dep['name']}</td>
                <td>{dep['version']}</td>
                <td>{dep['license']}</td>
                <td><a href="{dep['repository']}">{dep['repository']}</a></td>
            </tr>"""
        
        html += """
        </table>
    </div>
</body>
</html>"""
        
        return html

def main():
    parser = argparse.ArgumentParser(description='Audit licenses in Racing Wheel Suite dependencies')
    parser.add_argument('--format', choices=['json', 'csv', 'html'], default='json',
                       help='Output format (default: json)')
    parser.add_argument('--output', '-o', help='Output file (default: stdout)')
    parser.add_argument('--project-root', default='.', help='Project root directory')
    parser.add_argument('--fail-on-issues', action='store_true',
                       help='Exit with non-zero code if issues found')
    
    args = parser.parse_args()
    
    project_root = Path(args.project_root).resolve()
    auditor = LicenseAuditor(project_root)
    
    try:
        success = auditor.generate_report(args.format, args.output)
        
        if args.fail_on_issues and not success:
            print("License audit failed due to compliance issues")
            sys.exit(1)
        
        print("License audit completed successfully")
        
    except Exception as e:
        print(f"Error during license audit: {e}")
        sys.exit(1)

if __name__ == '__main__':
    main()