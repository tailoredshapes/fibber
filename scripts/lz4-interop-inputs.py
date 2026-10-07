#!/usr/bin/env python3
"""scripts/lz4-interop-inputs.py DIR: the generated inputs of scripts/lz4-interop.sh (empty, one byte, zeros, random, text)."""
import os, random, sys
w = sys.argv[1]
r = random.Random(7)
open(w + '/empty', 'wb').close()
open(w + '/one', 'wb').write(b'x')
open(w + '/zeros', 'wb').write(bytes(3000000))
open(w + '/random', 'wb').write(os.urandom(700000))
open(w + '/text', 'wb').write(b''.join(r.choice([b'alpha ', b'beta ', b'gamma\n', b'delta ', b'epsilon ']) for _ in range(400000)))
