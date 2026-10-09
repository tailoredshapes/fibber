#!/usr/bin/env python3
"""Check marked examples, local links and generated references.

The existing ADR runner remains responsible for fibber spec/fitness/measure
fences. This Python fallback handles documentation markers and uses the current
compiler and its case harness; it never guesses whether an unmarked sketch runs.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
MODES = {'run', 'check', 'frag', 'reject', 'case'}


def compiler_env():
    env = os.environ.copy()
    for name in list(env):
        if name.startswith(('FIB_', 'FIBBER_')):
            env.pop(name)
    env.update(FIB_LIB=str(ROOT / 'lib'), FIB_NO_PROJECT='1', FIB_THREADS='2')
    return env


def fences(path):
    lines = path.read_text().splitlines(keepends=True)
    blocks = []
    at = 0
    while at < len(lines):
        match = re.match(r'^ {0,3}(`{3,}|~{3,})(.*)\n?$', lines[at])
        if not match:
            at += 1
            continue
        start, delimiter, info = at + 1, match[1], match[2].strip()
        # Backticks in a backtick fence's info string are forbidden by Markdown;
        # this is inline code quoting a marker, not an opening fence.
        if delimiter[0] == '`' and '`' in info:
            at += 1
            continue
        at += 1
        body = []
        while at < len(lines) and not re.match(r'^ {0,3}' + re.escape(delimiter[0]) + '{' + str(len(delimiter)) + r',}\s*$', lines[at]):
            body.append(lines[at])
            at += 1
        if at == len(lines):
            raise ValueError(f'{path}:{start}: unclosed fence')
        blocks.append((start, info, ''.join(body)))
        at += 1
    return blocks


def examples(path):
    blocks = fences(path)
    found = []
    for i, (line, info, body) in enumerate(blocks):
        if info.split()[:1] != ['fib']:
            if info == 'text out' and (i == 0 or not blocks[i - 1][1].startswith('fib ')):
                raise ValueError(f'{path}:{line}: orphan text out')
            continue
        words = shlex.split(info)
        if len(words) < 2 or words[1] not in MODES or len(words) != (3 if words[1] == 'reject' else 2):
            raise ValueError(f'{path}:{line}: unknown marker {info!r}')
        expected = blocks[i + 1][2] if i + 1 < len(blocks) and blocks[i + 1][1] == 'text out' else None
        if expected is not None and words[1] != 'run':
            raise ValueError(f'{path}:{line}: text out requires fib run')
        found.append((path, line, words, body, expected))
    if re.search(r'^examples: required\s*$', path.read_text(), re.M) and not found:
        raise ValueError(f'{path}: examples required but none marked')
    if path.parent.name in {'tutorial', 'guide'}:
        unmarked = [line for line, info, body in blocks
                    if info in {'lisp', 'clojure', ''} and body.lstrip().startswith('(')]
        if unmarked:
            raise ValueError(f'{path}: unmarked language examples at {unmarked}')
    return found


def execute(example, fibc):
    path, line, words, body, expected = example
    label = f'{path.relative_to(ROOT)}:{line}' if path.is_relative_to(ROOT) else f'{path}:{line}'
    with tempfile.TemporaryDirectory(prefix='fibber-doc-') as scratch:
        work = Path(scratch)
        source = work / 'example.fib'
        mode = words[1]
        if mode == 'frag':
            body = '(ns main (:use fib.core fib.seq fib.coll fib.print fib.string))\n(defun main () -> i64 (do\n' + body + '\n0))\n'
        source.write_text(body)
        args = [fibc]
        if mode == 'case':
            args += ['cases', str(work)]
        else:
            args += ['-I', str(ROOT / 'lib')]
            if mode == 'run':
                args += ['run', '-O', '0', str(source)]
            else:
                args += ['build', str(source), '--emit', 'obj', '-o', str(work / 'example.o')]
        # Isolate project discovery and compiler control knobs inherited from a
        # contributor's shell. Every example sees this checkout's library.
        env = compiler_env()
        try:
            result = subprocess.run(args, cwd=work, env=env, capture_output=True, timeout=60)
        except subprocess.TimeoutExpired:
            return False, label + ': timed out'
        diagnostic = (result.stdout + result.stderr).decode(errors='replace')
        if mode == 'reject':
            ok = result.returncode == 3 and words[2] in diagnostic
        else:
            ok = result.returncode == 0
        if mode == 'case':
            ok = ok and result.stdout.endswith(b'1 cases: 1 pass, 0 fail, 0 pending, 0 header error\n')
        if ok and expected is not None:
            ok = result.stdout == expected.encode()
        if not ok:
            return False, f'{label}: {mode} failed (exit {result.returncode})\n{diagnostic[-3000:]}'
        return True, label


def link_errors(path):
    # Ignore examples, including Markdown sketches, before scanning prose links.
    source = path.read_text()
    for _, info, body in fences(path):
        source = source.replace(body, '')
    targets = re.findall(r'\]\(([^\s)]+)(?:\s+"[^"]*")?\)', source)
    targets += re.findall(r'^\s*\[[^\]]+\]:\s*(\S+)', source, re.M)
    errors = []
    for target in targets:
        target = target.strip('<>')
        url = urlsplit(target)
        if url.scheme or url.netloc or not url.path:
            continue
        resolved = (path.parent / unquote(url.path)).resolve()
        if not resolved.exists():
            errors.append(f'{path.relative_to(ROOT)}: broken link {target}')
    return errors


def smoke_examples(fibc, jobs):
    rows = json.loads((ROOT / 'examples/smoke.json').read_text())
    index = (ROOT / 'examples/README.md').read_text()
    errors = []

    def smoke(row):
        path = ROOT / row['source']
        if not path.is_file() or path.relative_to(ROOT / 'examples').as_posix() not in index:
            return f"example not indexed or missing: {row['source']}"
        with tempfile.TemporaryDirectory(prefix='fibber-smoke-') as scratch:
            args = [fibc, '-I', str(ROOT / 'lib'), '-I', str(ROOT / 'examples'),
                    '-I', str(ROOT / 'examples/gpu'), '-I', str(ROOT / 'examples/webgpu')]
            if row['mode'] == 'run':
                args += ['run', '-O', '2', str(path)]
                if row.get('args'):
                    args += ['--'] + [str(ROOT / arg) for arg in row['args']]
            elif row['mode'] == 'check':
                args += ['build', str(path), '--emit', 'obj', '-o', scratch + '/example.o']
            else:
                return f"unknown example smoke mode: {row['mode']}"
            try:
                result = subprocess.run(args, cwd=scratch, env=compiler_env(), capture_output=True, timeout=120)
            except subprocess.TimeoutExpired:
                return f"example timed out: {row['source']}"
            if result.returncode or (row['mode'] == 'run' and not result.stdout.endswith(b'0\n')):
                return f"example failed: {row['source']}\n" + (result.stdout + result.stderr).decode(errors='replace')[-2000:]
        return None

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        errors.extend(message for message in pool.map(smoke, rows) if message)
    print(f'examples: {len(rows)} smoke checks; {len(errors)} failures')
    return errors


def project_smoke(fibc):
    """Exercise the tutorial's generated-project commands with real resolution."""
    with tempfile.TemporaryDirectory(prefix='fibber-doc-project-') as scratch:
        work = Path(scratch)
        env = compiler_env()
        env.pop('FIB_NO_PROJECT')
        env['FIBBER_HOME'] = str(work / 'cache')
        commands = [(work, ['new', 'hello']),
                    (work / 'hello', ['run', 'src/main.fib']),
                    (work / 'hello', ['test']),
                    (work / 'hello', ['deps', 'tree']),
                    (work / 'hello', ['build', 'src/main.fib', '-o', 'hello'])]
        for cwd, words in commands:
            result = subprocess.run([fibc] + words, cwd=cwd, env=env, capture_output=True, timeout=60)
            if result.returncode:
                return ['tutorial project failed: ' + ' '.join(words) + '\n' +
                        (result.stdout + result.stderr).decode(errors='replace')[-2000:]]
        result = subprocess.run([str(work / 'hello/hello')], cwd=work, env=env, capture_output=True, timeout=60)
        if result.returncode or result.stdout != b'hello, world\n':
            return ['tutorial project executable failed']
    print('tutorial project: new, run, test, deps tree, build and executable held')
    return []


def paths():
    files = [ROOT / name for name in ['README.md', 'ROADMAP.md', 'CHANGELOG.md', 'CONTRIBUTING.md', 'SECURITY.md']]
    for base in ['docs', 'spec', 'lib', 'editors', 'examples', 'website']:
        files.extend((ROOT / base).rglob('*.md'))
    return sorted(set(files))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fibc', default=os.environ.get('FIBC', 'build/F'))
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--no-references', action='store_true', help='for isolated checker fault tests')
    parser.add_argument('paths', nargs='*', type=Path)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    fibc = str(Path(args.fibc).resolve())
    files = args.paths or paths()
    if args.quick:
        files = [p for p in files if p == ROOT / 'README.md' or p.parent == ROOT / 'docs/tutorial']
    rows = []
    errors = []
    unchecked = 0
    for path in files:
        try:
            marked = examples(path)
            rows.extend(marked)
            errors.extend(link_errors(path) if path.is_relative_to(ROOT) else [])
            unchecked += sum(info in {'', 'lisp', 'clojure'} and body.lstrip().startswith('(')
                             for _, info, body in fences(path))
        except ValueError as error:
            errors.append(str(error))
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for ok, message in pool.map(lambda row: execute(row, fibc), rows):
            if not ok:
                errors.append(message)
    if not args.no_references and not args.quick:
        result = subprocess.run(['python3', str(ROOT / 'scripts/doc-reference.py'), '--check', '--fibc', fibc], cwd=ROOT, capture_output=True, text=True)
        if result.returncode:
            errors.append(result.stdout + result.stderr)
        if not args.paths:
            errors.extend(smoke_examples(fibc, args.jobs))
            errors.extend(project_smoke(fibc))
    print(f'docs: {len(rows)} checked examples; {unchecked} unmarked sketches; {len(files)} files; {len(errors)} failures')
    if errors:
        raise SystemExit('\n'.join(errors))


if __name__ == '__main__':
    main()
