"""Publish existing Markdown without duplicating the language documentation."""
from pathlib import Path
import posixpath
import re
import shutil
import subprocess
import sys
from urllib.parse import quote, unquote, urlsplit, urlunsplit

from mkdocs.utils import get_relative_url

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'https://github.com/tailoredshapes/fibber/blob/main/'
SOURCES = {}
ROUTES = {}


def source_pages():
    pages = {'index.md': ROOT / 'website/index.md', 'project.md': ROOT / 'README.md'}
    for base in ['docs/tutorial', 'docs/guide', 'docs/reference', 'docs/policy']:
        for path in sorted((ROOT / base).rglob('*.md')):
            pages[path.relative_to(ROOT).as_posix()] = path
    for name in ['docs/README.md', 'examples/README.md', 'CONTRIBUTING.md',
                 'CHANGELOG.md', 'SECURITY.md', 'spec/syntax.md', 'spec/types.md',
                 'spec/ownership.md', 'spec/method.md']:
        pages[name] = ROOT / name
    for path in sorted((ROOT / 'lib/fib').rglob('README.md')):
        pages[path.relative_to(ROOT).as_posix()] = path
    return pages


def on_pre_build(config):
    staged = Path(config['docs_dir'])
    if staged.resolve() != ROOT / 'build/pages-source':
        raise ValueError('docs_dir must be the generated build/pages-source directory')
    SOURCES.clear()
    for route, source in source_pages().items():
        destination = staged / route
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.exists() or destination.read_bytes() != source.read_bytes():
            shutil.copyfile(source, destination)
        SOURCES[route] = source.relative_to(ROOT).as_posix()
    for source in (ROOT / 'website/assets').rglob('*'):
        if source.is_file():
            destination = staged / 'assets' / source.relative_to(ROOT / 'website/assets')
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or destination.read_bytes() != source.read_bytes():
                shutil.copyfile(source, destination)
    expected = set(SOURCES) | {'assets/' + p.relative_to(ROOT / 'website/assets').as_posix()
                              for p in (ROOT / 'website/assets').rglob('*') if p.is_file()}
    for stale in staged.rglob('*'):
        if stale.is_file() and stale.relative_to(staged).as_posix() not in expected:
            stale.unlink()


def on_files(files, config):
    ROUTES.clear()
    for route, source in SOURCES.items():
        file = files.get_file_from_path(route)
        if file is None:
            raise ValueError(f'missing published page: {route}')
        ROUTES[source] = file
    return files


def link_target(target, source, page, html=False):
    parsed = urlsplit(target)
    if parsed.scheme or parsed.netloc or not parsed.path or parsed.path.startswith('/'):
        return target
    path = posixpath.normpath(posixpath.join(posixpath.dirname(source), unquote(parsed.path)))
    if path.startswith('../') or not (ROOT / path).exists():
        raise ValueError(f'{source}: link destination is missing: {target}')
    if path in ROUTES:
        destination = ROUTES[path]
        relative = get_relative_url(destination.url if html else destination.src_uri,
                                    page.file.url if html else page.file.src_uri)
        return urlunsplit(('', '', relative, parsed.query, parsed.fragment))
    # Historical records and source/code downloads open on GitHub; only the
    # reader-facing docs are staged. Never copy compiler/runtime/build output.
    return REPOSITORY + quote(path, safe='/') + (('?' + parsed.query) if parsed.query else '') + (('#' + parsed.fragment) if parsed.fragment else '')


def on_page_markdown(markdown, page, config, files):
    source = SOURCES[page.file.src_uri]
    if source == 'website/index.md':
        markdown = markdown.replace('{{ release_version }}', (ROOT / 'VERSION').read_text().strip())
    lines = []
    fence = None
    for line in markdown.splitlines(keepends=True):
        marker = re.match(r'^ {0,3}(`{3,}|~{3,})([^\n]*)', line)
        if marker and '`' not in marker[2]:
            if fence is None:
                fence = marker[1]
                info = marker[2].strip()
                if info.startswith('fib '):
                    line = marker[1] + 'scheme\n'
                elif info == 'text out':
                    line = marker[1] + 'text\n'
            elif marker[1][0] == fence[0] and len(marker[1]) >= len(fence) and not marker[2].strip():
                fence = None
            lines.append(line)
            continue
        if fence is None:
            line = re.sub(r'(\]\()([^\s)]+)',
                          lambda m: m[1] + link_target(m[2], source, page), line)
            line = re.sub(r'((?:href|src)=")([^"]+)(")',
                          lambda m: m[1] + link_target(m[2], source, page, html=True) + m[3], line)
        lines.append(line)
    return ''.join(lines)


def on_post_build(config):
    Path(config['site_dir'], '.nojekyll').touch()
    subprocess.run([sys.executable, str(ROOT / 'website/check.py'), config['site_dir']], check=True)
