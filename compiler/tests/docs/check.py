#!/usr/bin/env python3
"""Planted failures for the documentation checker (uses a real compiler)."""
import argparse
import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys
from unittest.mock import patch

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('doc_examples', ROOT / 'scripts/doc-examples.py')
docs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(docs)
ref_spec = importlib.util.spec_from_file_location('doc_reference', ROOT / 'scripts/doc-reference.py')
refs = importlib.util.module_from_spec(ref_spec)
ref_spec.loader.exec_module(refs)


class Checks(unittest.TestCase):
    def document(self, text):
        scratch = tempfile.TemporaryDirectory(prefix='fibber-doc-fault-')
        self.addCleanup(scratch.cleanup)
        path = Path(scratch.name) / 'test.md'
        path.write_text(text)
        return path

    def run_document(self, text):
        rows = docs.examples(self.document(text))
        self.assertEqual(len(rows), 1)
        return docs.execute(rows[0], FIBC)[0]

    def test_stdout_fault(self):
        program = '```fib run\n(defun main () -> i64 (do (println "hello") 0))\n```\n'
        self.assertTrue(self.run_document(program + '```text out\nhello\n0\n```\n'))
        self.assertFalse(self.run_document(program + '```text out\nwrong\n0\n```\n'))

    def test_runtime_fault(self):
        self.assertFalse(self.run_document('```fib run\n(defun main () -> i64 (trap "planted"))\n```\n'))

    def test_check_does_not_run(self):
        self.assertTrue(self.run_document('```fib check\n(defun main () -> i64 (trap "must not run"))\n```\n'))

    def test_rejection_is_not_any_failure(self):
        rejected = '```fib reject "unbound name missing"\n(defun main () -> i64 missing)\n```\n'
        self.assertTrue(self.run_document(rejected))
        self.assertFalse(self.run_document(rejected.replace('unbound name missing', 'wrong diagnostic')))
        self.assertFalse(self.run_document('```fib reject "unbound name"\n(defun main () -> i64 0)\n```\n'))
        # Reader rejection is a legitimate frontend rejection too. A tool/usage
        # failure carrying the same text must still fail.
        path = self.document(rejected)
        stub = path.parent / 'bad-compiler'
        stub.write_text('#!/bin/sh\necho "unbound name missing"\nexit 2\n')
        stub.chmod(0o700)
        self.assertFalse(docs.execute(docs.examples(path)[0], str(stub))[0])

    def test_case_result_fault(self):
        case = '```fib case\n;; spec: docs\n;; expect: accept\n;; result: 7\n;; audit: clean\n(defun main () -> i64 7)\n```\n'
        self.assertTrue(self.run_document(case))
        self.assertFalse(self.run_document(case.replace('result: 7', 'result: 8')))

    def test_fragment(self):
        self.assertTrue(self.run_document('```fib frag\n(println (+ 1 2))\n```\n'))

    def test_hardway_spec_detects_broken_calculation(self):
        page = ROOT / 'docs/hardway/14-specs.md'
        program = docs.examples(page)[0]
        self.assertTrue(docs.execute(program, FIBC)[0])
        broken = list(program)
        broken[3] = broken[3].replace('(+ (* price quantity) delivery)',
                                    '(- (* price quantity) delivery)')
        self.assertNotEqual(program[3], broken[3])
        self.assertFalse(docs.execute(tuple(broken), FIBC)[0])

    def test_hardway_cli_detects_bad_exit_status(self):
        original = (ROOT / 'docs/hardway/16-capstone.md').read_text()
        broken = original.replace('(do (eprintln "usage: tally FILE") 2)',
                                  '(do (eprintln "usage: tally FILE") 0)')
        self.assertNotEqual(original, broken)
        errors = docs.hardway_smoke(FIBC, self.document(broken))
        self.assertTrue(any('no arguments' in error for error in errors), errors)
        self.assertTrue(any('two arguments' in error for error in errors), errors)

    def test_hardway_unmarked_program_rejected(self):
        path = self.document('```lisp\n(defun main () -> i64 0)\n```\n')
        folder = path.parent / 'hardway'
        folder.mkdir()
        course = folder / 'exercise.md'
        course.write_text(path.read_text())
        with self.assertRaisesRegex(ValueError, 'unmarked language examples'):
            docs.examples(course)

    def test_marker_faults(self):
        for text in ['```fib unknown\n0\n```\n',
                     '```fib reject\n0\n```\n',
                     '---\nexamples: required\n---\nno examples\n',
                     '```text out\norphan\n```\n',
                     '```fib run\nunclosed\n']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                docs.examples(self.document(text))

    def test_inline_marker_is_not_fence(self):
        self.assertEqual(docs.fences(self.document('   ```` ```fib run ```` is a marker.\n')), [])

    def test_link_fault(self):
        path = self.document('[exists](test.md) [missing](missing.md)')
        original = docs.ROOT
        try:
            docs.ROOT = path.parent
            errors = docs.link_errors(path)
        finally:
            docs.ROOT = original
        self.assertEqual(errors, ['test.md: broken link missing.md'])

    def test_reference_drift_and_unindexed_facade(self):
        path = self.document('fixture')
        root = path.parent
        for directory in ['lib/fib', 'compiler/types', 'rt']:
            (root / directory).mkdir(parents=True)
        (root / 'compiler/types/builtins.fib').write_text(
            '(BuiltinSig "cell" "(fn (a) (Cell a))" "" [EscStore] false)\n')
        (root / 'lib/fib/sample.fib').write_text(
            '(ns fib.sample)\n(defun visible () -> i64 1)\n'
            '(defun hidden :private () -> i64 2)\n')
        (root / 'rt/fixture.lir').write_text('"FIB_PLANTED"\n')
        with patch.object(refs, 'ROOT', root):
            pages = refs.references(FIBC)
            self.assertIn('FIB_PLANTED', pages['docs/reference/environment.md'])
            self.assertIn('visible', pages['docs/reference/library/fib.sample.md'])
            self.assertNotIn('hidden', pages['docs/reference/library/fib.sample.md'])
            for name, text in pages.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(text)
            with patch.object(sys, 'argv', ['references', '--check', '--fibc', FIBC]):
                refs.main()
                cli = root / 'docs/reference/cli.md'
                original = cli.read_text()
                cli.write_text(original + 'planted CLI drift\n')
                with self.assertRaises(SystemExit):
                    refs.main()
                cli.write_text(original)
                (root / 'lib/fib/new.fib').write_text('(ns fib.new)\n(defun added () -> i64 3)\n')
                with self.assertRaises(SystemExit):
                    refs.main()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--fibc', required=True)
    args = parser.parse_args()
    FIBC = str(Path(args.fibc).resolve())
    unittest.main(argv=['doc-checks'], verbosity=2)
