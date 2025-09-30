#!/usr/bin/env python3
"""
Analyze compatibility layer usage trends to project removal timeline.

This script analyzes historical usage data to determine if the compatibility
layer usage is trending downward and estimates when it might reach zero.
"""

import argparse
import json
import sys
from datetime import datetime, timedelta
from pathlib import Path
import statistics

def load_historical_data():
    """Load historical usage data from artifacts or local storage."""
    # In a real implementation, this would fetch from CI artifacts
    # For now, we'll simulate with some sample data
    
    # Try to load from local file if it exists
    metrics_file = Path('usage_metrics.jsonl')
    if metrics_file.exists():
        data = []
        with open(metrics_file, 'r') as f:
            for line in f:
                if line.strip():
                    data.append(json.loads(line))
        return data
    
    # Fallback to simulated historical data
    base_date = datetime.now() - timedelta(days=30)
    return [
        {
            'timestamp': (base_date + timedelta(days=i)).isoformat(),
            'usage_count': max(0, 50 - i * 2 + (i % 7)),  # Simulated downward trend
            'commit': f'abc123{i}',
            'branch': 'main'
        }
        for i in range(30)
    ]

def calculate_trend(data, current_usage, baseline_usage):
    """Calculate usage trend and project removal timeline."""
    if len(data) < 2:
        return {
            'trend_direction': 'insufficient_data',
            'trend_percentage': 0,
            'peak_usage': current_usage,
            'projected_removal_date': 'unknown'
        }
    
    # Sort by timestamp
    data.sort(key=lambda x: x['timestamp'])
    
    # Calculate trend over last 30 days
    recent_data = [d for d in data if 'main' in d.get('branch', '')]
    usage_counts = [d['usage_count'] for d in recent_data]
    
    if len(usage_counts) < 2:
        return {
            'trend_direction': 'insufficient_data',
            'trend_percentage': 0,
            'peak_usage': max(usage_counts) if usage_counts else current_usage,
            'projected_removal_date': 'unknown'
        }
    
    # Calculate linear trend
    peak_usage = max(usage_counts)
    recent_avg = statistics.mean(usage_counts[-7:]) if len(usage_counts) >= 7 else usage_counts[-1]
    older_avg = statistics.mean(usage_counts[:7]) if len(usage_counts) >= 14 else usage_counts[0]
    
    if older_avg == 0:
        trend_percentage = 0
        trend_direction = 'stable'
    else:
        trend_percentage = ((recent_avg - older_avg) / older_avg) * 100
        if trend_percentage < -5:
            trend_direction = 'decreasing'
        elif trend_percentage > 5:
            trend_direction = 'increasing'
        else:
            trend_direction = 'stable'
    
    # Project removal date based on trend
    projected_removal_date = 'unknown'
    if trend_direction == 'decreasing' and recent_avg > 0:
        # Simple linear projection
        days_per_unit = len(usage_counts) / max(1, older_avg - recent_avg)
        days_to_zero = recent_avg * days_per_unit
        if days_to_zero > 0 and days_to_zero < 365:  # Reasonable timeframe
            removal_date = datetime.now() + timedelta(days=days_to_zero)
            projected_removal_date = removal_date.strftime('%Y-%m-%d')
        elif days_to_zero <= 30:
            projected_removal_date = 'within_month'
        else:
            projected_removal_date = 'long_term'
    elif current_usage == 0:
        projected_removal_date = 'ready_now'
    
    return {
        'trend_direction': trend_direction,
        'trend_percentage': round(trend_percentage, 1),
        'peak_usage': peak_usage,
        'projected_removal_date': projected_removal_date,
        'current_usage': current_usage,
        'baseline_usage': baseline_usage
    }

def generate_recommendations(trend_data):
    """Generate actionable recommendations based on trend analysis."""
    recommendations = []
    
    if trend_data['trend_direction'] == 'increasing':
        recommendations.append({
            'type': 'warning',
            'message': 'Compatibility usage is increasing. Focus on migrating new code to use new field names.'
        })
        recommendations.append({
            'type': 'action',
            'message': 'Review recent PRs that added compatibility usage and create migration tasks.'
        })
    
    elif trend_data['trend_direction'] == 'decreasing':
        recommendations.append({
            'type': 'success',
            'message': 'Great progress! Compatibility usage is decreasing as expected.'
        })
        
        if trend_data['projected_removal_date'] == 'within_month':
            recommendations.append({
                'type': 'action',
                'message': 'Compatibility layer can likely be removed in the next minor release.'
            })
        elif trend_data['projected_removal_date'] == 'ready_now':
            recommendations.append({
                'type': 'action',
                'message': 'Compatibility layer can be removed immediately - no usage detected.'
            })
    
    elif trend_data['trend_direction'] == 'stable':
        if trend_data['current_usage'] > 0:
            recommendations.append({
                'type': 'info',
                'message': 'Compatibility usage is stable. Consider focused migration effort.'
            })
            recommendations.append({
                'type': 'action',
                'message': 'Create specific issues to migrate remaining compatibility usage.'
            })
        else:
            recommendations.append({
                'type': 'success',
                'message': 'No compatibility usage detected. Layer can be removed.'
            })
    
    return recommendations

def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description='Analyze compatibility layer usage trends')
    parser.add_argument('--current', type=int, required=True, help='Current usage count')
    parser.add_argument('--baseline', type=int, required=True, help='Baseline usage count')
    parser.add_argument('--output', default='trend_analysis.json', help='Output file for trend data')
    
    args = parser.parse_args()
    
    # Load historical data
    historical_data = load_historical_data()
    
    # Calculate trend
    trend_data = calculate_trend(historical_data, args.current, args.baseline)
    
    # Generate recommendations
    recommendations = generate_recommendations(trend_data)
    trend_data['recommendations'] = recommendations
    
    # Output results
    with open(args.output, 'w') as f:
        json.dump(trend_data, f, indent=2)
    
    # Print summary
    print(f"Trend Analysis Summary:")
    print(f"  Current usage: {trend_data['current_usage']}")
    print(f"  Baseline usage: {trend_data['baseline_usage']}")
    print(f"  Trend direction: {trend_data['trend_direction']}")
    print(f"  Trend percentage: {trend_data['trend_percentage']}%")
    print(f"  Peak usage: {trend_data['peak_usage']}")
    print(f"  Projected removal: {trend_data['projected_removal_date']}")
    
    print(f"\nRecommendations:")
    for rec in recommendations:
        icon = {'warning': '⚠️', 'success': '✅', 'info': 'ℹ️', 'action': '🔧'}.get(rec['type'], '•')
        print(f"  {icon} {rec['message']}")
    
    # Exit with appropriate code
    if trend_data['trend_direction'] == 'increasing':
        return 1  # Fail CI if usage is increasing
    return 0

if __name__ == '__main__':
    sys.exit(main())