#!/usr/bin/env python3
"""Refuse empty, skipped or inconsistent compiled-spec success reports."""
import json
from pathlib import Path
import sys


def main():
    report = json.loads(Path(sys.argv[1]).read_text())
    summary = report['summary']
    total = summary['total']
    if total <= 0 or summary['pass'] != total:
        raise RuntimeError('empty or unsuccessful scenario report')
    if any(summary.get(key, 0) != 0 for key in ['fail', 'trap', 'timeout', 'skip']):
        raise RuntimeError('failed, trapped, timed out or skipped scenario')
    if len(report['results']) != total or any(row['status'] != 'pass' for row in report['results']):
        raise RuntimeError('missing or non-passing scenario rows')
    print('PASS ' + str(total) + ' compiled scenarios')


if __name__ == '__main__':
    main()
