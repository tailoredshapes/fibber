#!/usr/bin/env python3
"""Planted failures for site links and the source-to-site publishing adapter."""
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import check as checker
import hooks


class PublishedLinks(unittest.TestCase):
    def setUp(self):
        self.temp = TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.site = Path(self.temp.name)
        self.write('index.html', '<a href="guide/#hello">Guide</a>')
        self.write('guide/index.html', '<h1 id="hello">Hello</h1><a href="/fibber/">Home</a>')
        self.write('search/search_index.json', '{}')

    def write(self, name, text):
        path = self.site / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def errors(self, html):
        self.write('index.html', html)
        return checker.check(self.site)[2]

    def test_relative_links_and_project_base(self):
        self.assertEqual(checker.check(self.site)[2], [])

    def test_missing_page(self):
        self.assertIn('missing link or asset', self.errors('<a href="missing/">Missing</a>')[0])

    def test_missing_anchor(self):
        self.assertIn('missing anchor', self.errors('<a href="guide/#absent">Missing</a>')[0])

    def test_missing_script(self):
        self.assertIn('missing link or asset', self.errors('<script src="assets/missing.js"></script>')[0])

    def test_wrong_project_base(self):
        self.assertIn('escapes project base path', self.errors('<a href="/guide/">Guide</a>')[0])

    def test_output_escape(self):
        self.assertIn('leaves output tree', self.errors('<a href="../outside.html">Outside</a>')[0])

    def test_external_links_are_not_local_files(self):
        self.assertEqual(self.errors('<a href="https://github.com/x/y">Source</a><a href="mailto:dev@example.com">Mail</a>'), [])

    def test_search_index_required(self):
        (self.site / 'search/search_index.json').unlink()
        self.assertIn('homepage or search index is missing', checker.check(self.site)[2])


class SourceAdapter(unittest.TestCase):
    def setUp(self):
        self.page = SimpleNamespace(file=SimpleNamespace(src_uri='docs/guide/tooling.md', url='docs/guide/tooling/'))
        self.routes = {'docs/tutorial/01-install-and-hello.md': SimpleNamespace(
            src_uri='docs/tutorial/01-install-and-hello.md', url='docs/tutorial/01-install-and-hello/')}

    def test_reader_link_preserves_anchor(self):
        with patch.dict(hooks.ROUTES, self.routes, clear=True):
            result = hooks.link_target('../tutorial/01-install-and-hello.md#platform-setup-and-release-layout',
                                       'docs/guide/tooling.md', self.page)
            self.assertEqual(result, '../tutorial/01-install-and-hello.md#platform-setup-and-release-layout')
            html = hooks.link_target('../tutorial/01-install-and-hello.md#platform-setup-and-release-layout',
                                     'docs/guide/tooling.md', self.page, html=True)
            self.assertEqual(html, '../../tutorial/01-install-and-hello/#platform-setup-and-release-layout')

    def test_source_link_opens_repository(self):
        with patch.dict(hooks.ROUTES, self.routes, clear=True):
            self.assertEqual(hooks.link_target('../../compiler/fibc.fib', 'docs/guide/tooling.md', self.page),
                             hooks.REPOSITORY + 'compiler/fibc.fib')

    def test_missing_source_rejected(self):
        with self.assertRaisesRegex(ValueError, 'destination is missing'):
            hooks.link_target('../../missing-page.md', 'docs/guide/tooling.md', self.page)

    def test_code_literals_are_not_rewritten(self):
        source = '```fib reject "diagnostic"\n[example](missing.md)\n```\n```text out\noutput\n```\n'
        with patch.dict(hooks.SOURCES, {self.page.file.src_uri: 'docs/guide/tooling.md'}, clear=True):
            result = hooks.on_page_markdown(source, self.page, {}, [])
        self.assertEqual(result, '```scheme\n[example](missing.md)\n```\n```text\noutput\n```\n')

    def test_homepage_uses_current_release(self):
        page = SimpleNamespace(file=SimpleNamespace(src_uri='index.md', url=''))
        with patch.dict(hooks.SOURCES, {'index.md': 'website/index.md'}, clear=True):
            rendered = hooks.on_page_markdown('Release {{ release_version }}', page, {}, [])
        self.assertEqual(rendered, 'Release ' + (hooks.ROOT / 'VERSION').read_text().strip())

    def test_staging_cannot_overwrite_source_directory(self):
        with self.assertRaisesRegex(ValueError, 'generated build/pages-source'):
            hooks.on_pre_build({'docs_dir': str(hooks.ROOT / 'docs')})


if __name__ == '__main__':
    unittest.main(verbosity=2)
