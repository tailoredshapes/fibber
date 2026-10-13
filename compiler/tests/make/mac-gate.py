#!/usr/bin/env python3
"""Check the Mac launcher using fake hardware and Make, without compiling."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class MacGate(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="fibber-mac-gate-")
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.calls = self.base / "calls.jsonl"
        for name, body in {
            "uname": "#!/bin/sh\nprintf '%s\\n' Darwin\n",
            "sysctl": '''#!/bin/sh
case "$2" in
  hw.perflevel0.physicalcpu) [ "$PERF" != missing ] || exit 1; echo "$PERF" ;;
  hw.physicalcpu) echo "$PHYSICAL" ;;
  hw.memsize) echo "$MEMORY" ;;
  *) exit 1 ;;
esac
''',
            "gmake": '''#!/usr/bin/env python3
import json, os, sys
with open(os.environ['CALLS'], 'a') as f:
    f.write(json.dumps({'args': sys.argv[1:], 'cwd': os.getcwd(),
                       'workers': [os.environ.get(n) for n in
                                   ('CASE_JOBS', 'CHECK_JOBS', 'UNITS_JOBS')]}) + '\\n')
sys.exit(int(os.environ.get('REPORT_STATUS' if 'mac-report' in sys.argv else 'STAMPS_STATUS', '0')))
''',
        }.items():
            p = self.base / name
            p.write_text(body)
            p.chmod(0o755)

    def run_gate(self, *args, **extra):
        env = os.environ.copy()
        for name in ('GATE_BUDGET', 'GATE_ROOT', 'BUILD', 'SCRATCH', 'BUILDER',
                     'MAC_QUICK', 'LIMIT_STDLIB', 'LIMIT_OWNERSHIP', 'CASE_JOBS', 'CHECK_JOBS', 'UNITS_JOBS'):
            env.pop(name, None)
        env.update(PATH=f"{self.base}:{env['PATH']}", MAKE=str(self.base / 'gmake'),
                   PERF='16', PHYSICAL='20', MEMORY=str(64 * 1024**3),
                   CALLS=str(self.calls), FIBSUITE_LOCKED='1')
        env.update(extra)
        return subprocess.run(['bash', str(ROOT / 'scripts/mac-gate.sh'), *args],
                              env=env, text=True, capture_output=True)

    def test_m1_ultra_plan_does_not_run_make(self):
        result = self.run_gate('--plan')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('12 Make jobs', result.stdout)
        self.assertIn('-j12 mac-stamps', result.stdout)
        self.assertFalse(self.calls.exists())

    def test_smaller_machines_and_override(self):
        for env, jobs in (({'MEMORY': str(24 * 1024**3)}, 2),
                          ({'MEMORY': str(8 * 1024**3)}, 1),
                          ({'PERF': 'missing', 'PHYSICAL': '4'}, 4),
                          ({'GATE_BUDGET': '8'}, 8)):
            with self.subTest(env=env):
                result = self.run_gate('--plan', **env)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f'-j{jobs} mac-stamps', result.stdout)

    def test_invalid_budget_does_not_run_make(self):
        for value in ('', '0', '-1', 'eight'):
            result = self.run_gate(GATE_BUDGET=value)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertFalse(self.calls.exists())

    def test_quick_gate_preserves_paths_and_bounds_nested_workers(self):
        result = self.run_gate('--quick', BUILD='build with spaces',
                               SCRATCH=str(self.base / 'scratch with spaces'))
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0]['args'][:3], ['-k', '-j12', 'mac-stamps'])
        self.assertEqual(calls[1]['args'][:2], ['--no-print-directory', 'mac-report'])
        for call in calls:
            self.assertEqual(call['cwd'], str(ROOT))
            self.assertEqual(call['workers'], ['4', '2', '1'])
            self.assertIn('BUILD=build with spaces', call['args'])
            self.assertIn('MAC_QUICK=1', call['args'])

    def test_failed_stamps_still_report_and_fail(self):
        result = self.run_gate(STAMPS_STATUS='2', REPORT_STATUS='1')
        self.assertEqual(result.returncode, 1, result.stderr)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertEqual(len(calls), 2)
        self.assertIn('mac-report', calls[1]['args'])

    def test_full_gate_overrides_inherited_quick_mode(self):
        result = self.run_gate('--full', MAC_QUICK='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        for call in calls:
            self.assertIn('MAC_QUICK=', call['args'])


if __name__ == '__main__':
    unittest.main()
