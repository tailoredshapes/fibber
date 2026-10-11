"""Source mutations prove the behavioral checks notice broken reuse contracts."""
from pathlib import Path
import shutil

from check import build_probe, cache_checks, artifact_checks


PLANTS = {
    "lookup-disabled": [("native/liro/compile.fib",
                         '(if (= (. c cache) "") (Err "cache disabled")',
                         '(if true (Err "cache disabled")')],
    "stale-body-key": [("native/liro/compile.fib",
                        '(str contract "\\n" (print/print-module m))',
                        '(str contract "\\n" "body omitted")')],
    "optimization-omitted": [("native/liro/context.fib", '"\\nopt=" opt', '"\\nopt=" 2'),
                             ("native/liro/context.fib", '"relocation=pic\\npipeline=default<O" opt',
                              '"relocation=pic\\npipeline=default<O" 2')],
    "abi-check-disabled": [("native/liro/link.fib", '(not (contract/same-abi? it owner))', 'false')],
    "checksum-disabled": [("native/liro/file.fib", '(if (= actual digest)', '(if true')],
}


def check_plants(fibc, scratch, env, root):
    for name, patches in PLANTS.items():
        directory = scratch / name
        directory.mkdir()
        compiler = directory / "compiler"
        shutil.copytree(root / "compiler", compiler,
                        ignore=shutil.ignore_patterns("tests", "__pycache__"))
        for relative, before, after in patches:
            source = compiler / relative
            text = source.read_text()
            assert text.count(before) == 1, (name, relative, before)
            source.write_text(text.replace(before, after))
        probe = directory / "api"
        changed_env = dict(env, FIB_LIRO_CACHE=str(directory / "cache"))
        build_probe(fibc, probe, changed_env, compiler)
        test = artifact_checks if name in {"abi-check-disabled", "checksum-disabled"} else cache_checks
        try:
            test(probe, directory, changed_env)
        except AssertionError:
            print(f"ok planted {name}: behavioral check fails")
        else:
            raise AssertionError(f"plant {name} survived the checks")
