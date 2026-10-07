#!/usr/bin/env python3
# Generates the cases of the fib.regex differential test (scripts/regex-diff/run.sh): lines `MODE HEX(pattern) HEX(input)` with MODE f (find) or
# m (matches), the UTF-8 bytes in hex so that no escaping is needed. Patterns come from a small grammar inside the subset fib.regex runs with
# Java's meaning (literals, classes, the escapes, groups, alternation, greedy and lazy quantifiers, anchors, \b, the inline flags i s m x,
# single-digit backreferences, \x{..}, and with --unicode the \p{..} properties of fib.regex.unicode). Fixed seed: the same seed is the same file.
# Usage: gen.py SEED COUNT [--unicode]
import random, sys

seed, count = int(sys.argv[1]), int(sys.argv[2])
unicode = '--unicode' in sys.argv
r = random.Random(seed)

LITS = list("abcABC01 _-x") + ['é', 'ß', 'λ', '٣', '😀']
INPUT_CHARS = list("abcABC01 _-xy.") + ['\n', '\r\n', '\t', 'é', 'É', 'λ', 'Λ', '٣', 'ß']
CLASSES = ['[a-c]', '[^a-c]', '[A-C0-1]', '[\\w-]', '[a\\d]', '[^\\s]', '[x-]', '[\\x{e9}-\\x{ff}]', '[a-cA-C]', '[^\\d\\n]']
ESCAPES = ['\\d', '\\w', '\\s', '\\D', '\\W', '\\S', '\\x{41}', '\\t', '\\.', '\\-']
PROPS = ['\\p{L}', '\\p{Lu}', '\\p{Ll}', '\\p{N}', '\\p{Nd}', '\\P{L}', '\\p{IsGreek}', '\\p{IsLatin}', '\\p{script=Greek}', '\\p{Alpha}', '\\p{Punct}',
         '[\\p{L}\\d]', '[^\\p{Lu}]', '[\\P{Nd}a]', '\\p{Zs}', '\\p{Sc}', '\\p{M}', '\\p{gc=Ll}', '\\p{IsL}']

def lit():
    return r.choice(LITS)

def atom(depth, groups):
    k = r.random()
    if k < 0.34:
        return lit()
    if k < 0.42:
        return '.'
    if k < 0.52:
        return r.choice(ESCAPES)
    if k < 0.62:
        return r.choice(CLASSES)
    if unicode and k < 0.70:
        return r.choice(PROPS)
    if k < 0.76 and groups[0] > 0:
        return '\\%d' % r.randint(1, min(groups[0], 9))
    if k < 0.80 and depth == 0:
        return r.choice(['^', '$', '\\b', '\\B', '\\A', '\\z', '\\Z'])
    if depth < 3:
        if r.random() < 0.7:
            groups[0] += 1
            body = alt(depth + 1, groups)
            return '(' + body + ')'
        return '(?:' + alt(depth + 1, groups) + ')'
    return lit()

def quant():
    k = r.random()
    if k < 0.55:
        return ''
    q = r.choice(['*', '+', '?', '{2}', '{1,2}', '{0,3}', '{2,}'])
    if r.random() < 0.25:
        q += '?'
    return q

def piece(depth, groups):
    a = atom(depth, groups)
    if a in ('^', '$', '\\b', '\\B', '\\A', '\\z', '\\Z'):
        return a
    return a + quant()

def cat(depth, groups):
    return ''.join(piece(depth, groups) for _ in range(r.randint(1, 4)))

def alt(depth, groups):
    n = 1 if r.random() < 0.75 else r.randint(2, 3)
    return '|'.join(cat(depth, groups) for _ in range(n))

def pattern():
    groups = [0]
    p = alt(0, groups)
    f = r.random()
    if f < 0.10: p = '(?i)' + p
    elif f < 0.16: p = '(?s)' + p
    elif f < 0.24: p = '(?m)' + p
    elif f < 0.28: p = '(?ims)' + p
    elif f < 0.31: p = '(?i-s)' + p
    elif f < 0.34: p = '(?x)' + p.replace(' ', '_').replace('a', ' a')
    elif f < 0.37:
        i = r.randint(0, len(p))
        ok = not p[:i].endswith('\\') and (i == len(p) or p[i] not in '*+?{') and (i == 0 or p[i - 1] not in '(|')
        p = p[:i] + '(?i)' + p[i:] if ok else p
    return p

def text(p):
    # a third of the texts are drawn from the characters the pattern itself names, so that matches are common (without it four in five searches find nothing)
    pool = [c for c in p if c.isalnum() or c in ' _-.é'] if r.random() < 0.6 else []
    chars = pool + INPUT_CHARS[:6] if pool else INPUT_CHARS
    return ''.join(r.choice(chars) for _ in range(r.randint(0, 9)))

out = []
for _ in range(count):
    p = pattern()
    for _ in range(3):
        mode = 'f' if r.random() < 0.7 else 'm'
        out.append('%s %s %s' % (mode, p.encode().hex() or '-', text(p).encode().hex() or '-'))
print('\n'.join(out))
