import sys, yaml
# usage: remarks.py r.yaml [funcprefix]
pre = sys.argv[2] if len(sys.argv) > 2 else 'f.'
for d in yaml.load_all(open(sys.argv[1]), Loader=yaml.BaseLoader):
    if not d or d.get('Pass') != 'loop-vectorize' or not d['Function'].startswith(pre):
        continue
    parts = []
    for x in d['Args']:
        for k, v in x.items():
            if k in ('DebugLoc',):
                continue
            parts.append(str(v))
    print(d['Name'], d['Function'], '|', ''.join(parts))
