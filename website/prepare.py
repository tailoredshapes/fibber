#!/usr/bin/env python3
"""Stage generated MkDocs inputs before its configuration validates docs_dir."""
import sys
sys.dont_write_bytecode = True
from hooks import ROOT, on_pre_build

if __name__ == '__main__':
    on_pre_build({'docs_dir': str(ROOT / 'build/pages-source')})
