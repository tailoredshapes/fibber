# GitHub Pages site

The public site is built from the existing tutorial, Hard Way course, guides, references, library
READMEs and selected specification chapters. `index.md` supplies the adoption
page; the compiler's VERSION supplies its release links. Original Markdown
remains in place and its executable markers remain checked by `make doc-examples`.

```sh
python3 -m venv /tmp/fibber-pages
/tmp/fibber-pages/bin/pip install -r website/requirements.txt
make docs-site DOCS_PYTHON=/tmp/fibber-pages/bin/python
make docs-serve DOCS_PYTHON=/tmp/fibber-pages/bin/python
```

Open `http://127.0.0.1:8000/fibber/` for the local preview. The site build needs
no LLVM or compiler bootstrap. Generated input/output stays under `build/`.
`hooks.py` stages only reader-facing pages, preserves internal links, sends
source and historical links to GitHub, and renders the executable Fibber blocks
with Lisp highlighting. `check.py` validates every generated local destination,
anchor and asset; it checks the project subpath as well as the homepage/search.

`.github/workflows/pages.yml` builds on documentation pull requests and publishes
pushes to `main` through the official Pages artifact/deploy actions. The repository
Pages publishing source must be **GitHub Actions**. A `gh-pages` branch is not used.
The Pages job is separate from compiler CI so documentation publishing does not
require downloading LLVM or rerunning self-compilation. To force a rebuild, run
the `pages` workflow manually on main.
