#!/usr/bin/env python3
"""Builds the 100 MB NDJSON benchmark file: the three corpora (twitter, citm_catalog, canada), each minified to one line, repeated round robin until the file
has about 100 MB. usage: make-ndjson.py CORPUS_DIR OUT [MB]"""
import json, sys
d, out = sys.argv[1], sys.argv[2]
mb = int(sys.argv[3]) if len(sys.argv) > 3 else 100
lines = [json.dumps(json.load(open('%s/%s.json' % (d, n))), separators=(',', ':'), ensure_ascii=False) + '\n' for n in ('twitter', 'citm_catalog', 'canada')]
total = 0
with open(out, 'w', encoding='utf-8') as f:
    i = 0
    while total < mb * 1000000:
        f.write(lines[i % 3]); total += len(lines[i % 3].encode()); i += 1
print('wrote', out, total, 'bytes,', i, 'lines')
