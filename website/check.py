#!/usr/bin/env python3
"""Validate generated HTML links and assets, including the /fibber/ base path."""
import argparse
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


class Page(HTMLParser):
    def __init__(self, text):
        super().__init__()
        self.ids = set()
        self.links = []
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        if attributes.get('id'):
            self.ids.add(attributes['id'])
        if tag in {'a', 'link', 'script', 'img'}:
            target = attributes.get('href' if tag in {'a', 'link'} else 'src')
            if target:
                self.links.append(target)


def check(site):
    site = Path(site).resolve()
    documents = {p: Page(p.read_text()) for p in site.rglob('*.html')}
    errors = []
    checked = 0
    for path, document in documents.items():
        for target in document.links:
            url = urlsplit(target)
            if url.scheme or url.netloc:
                continue
            checked += 1
            if url.path.startswith('/'):
                if not url.path.startswith('/fibber/'):
                    errors.append(f'{path.relative_to(site)}: link escapes project base path: {target}')
                    continue
                destination = site / unquote(url.path[len('/fibber/'):])
            else:
                destination = path.parent / unquote(url.path) if url.path else path
            destination = destination.resolve()
            if not destination.is_relative_to(site):
                errors.append(f'{path.relative_to(site)}: link leaves output tree: {target}')
                continue
            if destination.is_dir():
                destination /= 'index.html'
            if not destination.is_file():
                errors.append(f'{path.relative_to(site)}: missing link or asset: {target}')
            elif url.fragment and destination in documents and unquote(url.fragment) not in documents[destination].ids:
                errors.append(f'{path.relative_to(site)}: missing anchor: {target}')
    if not (site / 'index.html').is_file() or not (site / 'search/search_index.json').is_file():
        errors.append('homepage or search index is missing')
    return len(documents), checked, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('site', nargs='?', default='build/pages')
    args = parser.parse_args()
    pages, links, errors = check(args.site)
    print(f'site: {pages} HTML pages, {links} local links/assets, {len(errors)} failures')
    if errors:
        raise SystemExit('\n'.join(errors))


if __name__ == '__main__':
    main()
