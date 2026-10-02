#!/usr/bin/env python3
"""Writes lower-body-cases/NAME.fib and NAME.out (package P7's goldens).

usage: lower-body-gen.py FIBREF [NAME..]   (run from the repository root)
The .out is `FIBREF types --stage lower --ast --sections ast,error` of the case, its `ast fun f`
block with every E and B id made relative to the first of the block, or its `error` lines."""
import re, subprocess, sys, os

D = "compiler/tests/types/lower-body-cases/"
HEAD = """(defstruct Pt (px: i64 py: i64))
(defenum Shape (Circle r: i64) (Square s: i64) Empty)
(defun g (a b) a)
(defun h (a) a)
"""
CASES = {
 "atoms": "(defun f () (do 1 2.5 \"s\" \\a true :kw () nil))",
 "if": "(defun f (x: i64 y: i64) -> i64 (if (< x y) x (+ y 1)))",
 "do-empty": "(defun f () (do))",
 "do-one": "(defun f (x) (do x))",
 "body-many": "(defun f (x) (g x 1) (g x 2) (h x))",
 "let": "(defun f (x: i64) -> i64 (let ((a 1) (b: i64 2) ((some z) (g x 1)) ([p q] (g x 2))) (+ a (+ b z))))",
 "let-nested": "(defun f (x) (let ((a 1)) (let ((b a) ((Pt c d) x)) (g a (g b (g c d))))))",
 "match": "(defun f (x) (match x ((some a) :when (> a 0) a) (nil 0) (_ 2)))",
 "match-enum": "(defun f (s: Shape) (match s ((Circle r) r) ((Square q) q) ((Empty) 0)))",
 "match-struct": "(defun f (p: Pt) (match p ((Pt a b) (+ a b))))",
 "match-vec": "(defun f (v) (match v ([] 0) ([a] a) ([a b & r] b) ([& _] 1)))",
 "match-as-lit": "(defun f (x) (match x (1 :a) (\\c :b) (\"s\" :c) (true :d) ((p :as w) w)))",
 "loop": "(defun f () (loop ((i 0) ([a b] (g 1 2))) (if (< i 3) (recur (+ i 1) (g a b)) i)))",
 "loop-ann": "(defun f () (loop ((i: i64 0)) i))",
 "fn": "(defun f (x: i64) (fn self (a b: i64) -> i64 (let ((c (g a b))) (+ c x))))",
 "fn-captures": "(defun f (x) (fn (a) (fn (b) (g a (g b x)))))",
 "async": "(defun f (x) (async (await (g x 1))))",
 "async-empty": "(defun f () (do (async) (unsafe)))",
 "unsafe": "(defun f (x) (unsafe (release-raw x)))",
 "field": "(defun f (p: Pt) (. (g p 1) py))",
 "quote": "(defun f () (quote (a (b c) [d] 1 \"s\")))",
 "var": "(defun f () (var g))",
 "amp": "(defun f ((& c) y) (do (set! c (+ (deref c) y)) (g (& c) y)))",
 "set-field": "(defun f ((& s) y) (set-field! (& s) px y))",
 "concat": "(defun f (x) (concat \"a\" x \"c\"))",
 "dyn": "(defun f (x) (do (dyn Show x) (dyn Show :send x)))",
 "convert": "(defun f (x: i64 y: f64) (do (trunc i32 x) (sitofp f64 x) (fptosi i32 y) (fpext f64 y)))",
 "err-bound-twice": "(defun f (x) (match x ((Pt a a) a)))",
 "err-recur-outside": "(defun f () (recur 1))",
 "err-recur-tail": "(defun f () (loop ((i 0)) (do (recur 1) 2)))",
 "err-await": "(defun f (x) (await x))",
 "err-await-arity": "(defun f (x) (async (await x x)))",
 "err-underscore": "(defun f (x) (g _ 1))",
 "err-unbound": "(defun f (x) (g nosuch 1))",
 "err-fn-body": "(defun f () (fn (a)))",
 "err-fn-params": "(defun f () (fn a))",
 "err-fn-repeat": "(defun f () (fn (a a) a))",
 "err-fn-param-form": "(defun f () (fn (_) 1))",
 "err-let-shape": "(defun f () (let (a 1) a))",
 "err-let-body": "(defun f () (let ((a 1))))",
 "err-let-annot": "(defun f () (let ((_: i64 3)) 1))",
 "err-quote": "(defun f () (quote))",
 "err-amp-expr": "(defun f (x) (& x))",
 "err-ctor": "(defun f (x) (match x ((Nosuch a) a)))",
 "err-ctor-arity": "(defun f (x) (match x ((Pt a) a)))",
 "err-vec-rest": "(defun f (x) (match x ([& a b] 1)))",
 "err-vec-amp-pair": "(defun f (x) (match x ([(& r)] 1)))",
 "err-amp-value": "(defun f ((& c)) c)",
 "err-unsafe": "(defun f (x) (release-raw x))",
 "err-conv-target": "(defun f (x) (trunc f64 x))",
 "err-conv-form": "(defun f (x) (trunc (Vec i64) x))",
 "err-conv-arity": "(defun f (x) (trunc i32))",
 "err-set-field": "(defun f (x) (set-field! x px 1))",
 "err-if": "(defun f () (if 1 2))",
 "err-prim-value": "(defun f () (g trunc 1))",
 "err-deref-arity": "(defun f ((& c)) (deref c c))",
}

# Cases the expander rejects before the lowering can see them (the oracle prints the expander's
# record): the position is the oracle's, the kind and message are those of the Rust lowering code
# (lower/*.rs), which the harness reaches because it does not expand.
LOWER_ONLY = {
 "err-amp-expr": ("AmpArgument", "& argument must be a cell variable"),
 "err-await-arity": ("Resolve", "await takes one operand"),
 "err-fn-body": ("Resolve", "fn needs a body"),
 "err-fn-params": ("Resolve", "fn needs a parameter list"),
 "err-if": ("Resolve", "if takes a test and two branches"),
 "err-let-body": ("Resolve", "let needs bindings and a body"),
 "err-let-shape": ("Resolve", "a binding is (pattern expression) or (name: type expression)"),
 "err-quote": ("Resolve", "malformed quote"),
}

def rebase(block):
    ids = {}
    for kind in "EB":
        nums = [int(n) for l in block for n in re.findall(r"\b" + kind + r"(\d+)\b", l)]
        ids[kind] = min(nums) if nums else 0
    sub = lambda m: m.group(1) + str(int(m.group(2)) - ids[m.group(1)])
    return [re.sub(r"\b([EB])(\d+)\b", sub, l) for l in block]

def out_of(fibref, path):
    r = subprocess.run([fibref, "types", "--stage", "lower", "--ast", "--sections", "ast,error", path],
                       capture_output=True, text=True)
    lines = r.stdout.split("\n")
    block, on = [], False
    for l in lines:
        if l.startswith("ast fun f") and l.strip() == "ast fun f":
            on = True; block = [l]; continue
        if on and (l.startswith("ast ") or l.startswith("-- ") or l.startswith("== ")):
            on = False
        elif on:
            block.append(l)
    block = [l for l in block if l != ""]
    if block:
        return rebase(block)
    return [l for l in lines[1:] if l != ""]

def lowered_only(n, lines):
    m = re.match(r"error \w+ (\S+ \S+): ", lines[-1])
    kind, msg = LOWER_ONLY[n]
    return ["error %s %s: %s" % (kind, m.group(1), msg)]

def main():
    fibref, names = sys.argv[1], sys.argv[2:] or list(CASES)
    os.makedirs(D, exist_ok=True)
    for n in names:
        path = D + n + ".fib"
        open(path, "w").write(HEAD + CASES[n] + "\n(defun main () -> i64 0)\n")
        out = out_of(fibref, path)
        if n in LOWER_ONLY:
            out = lowered_only(n, out)
        open(D + n + ".out", "w").write("\n".join(out) + "\n")

main()
