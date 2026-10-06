# Projects and git dependencies: fibber's first package manager

Status: MVP implemented (compiler/pkg/, driver wiring in compiler/fibc.fib and compiler/driver/args.fib); tests in
compiler/tests/deps/run.sh, planted faults in scripts/mutant-deps.sh. Date: 2026-10-06.

The owner asked for "a package manager for libraries", using git "as an early system". The tie-breaker is the project rule: Clojure's
ergonomics unless they break memory safety, then Rust's. For a package manager the two references are Clojure's tools.deps (`deps.edn`,
`:git/url` + `:git/sha`/`:git/tag`, `:local/root`, aliases, a cache of git checkouts) and Rust's Cargo (`Cargo.lock`, reproducible and
offline builds, `--locked`). This design takes **deps.edn's shape and Cargo's lockfile and reproducibility**. Nothing here touches memory
safety, so where the two differ the choice is ergonomic, and each one is argued below.

## 0. The short answer

1. A project is a directory with a `deps.fib`: fibber data (one map) in deps.edn's shape. `:paths` are its source roots, `:deps` maps a
   library name to a git commit (`:git/url` with `:git/sha` and/or `:git/tag`) or a directory (`:local/root`).
2. `deps.lock` (fibber data, written by fibc, committed) records the root's requests and every resolved library: URL, commit, the
   commit's **tree id** (git's own hash of the files), paths, the names it depends on, and who asked for it.
3. Resolution is breadth first over the `deps.fib` files of the libraries (read as data; no library code runs). The same library at two
   different commits is a **CONFLICT** that names both chains of requests; the root's `:override` is the only way to choose. A tag is
   resolved to a commit once and the commit is locked; a tag that moved is refused until `fibc deps update NAME`.
4. Checkouts live in `$FIBBER_HOME/git/HOST/PATH/SHA` (default `~/.cache/fibber`), fetched shallow, read-only, verified (commit, tree,
   clean files) on every build, made by an atomic rename so concurrent builds are safe.
5. `fibc new NAME [--lib]`; `fibc build|run|test|emit|explain` find `deps.fib` from the working directory upwards and add the project's
   roots and every library's roots to the module search path (no `-I`); `fibc deps tree|fetch|update|add|path`. `--locked`, `--offline`,
   `--frozen` (both) as Cargo.
6. **A dependency runs code at compile time**: its macros are JIT-compiled and executed by `fibc build` (like Rust's proc-macros and
   build.rs). Resolution itself only runs `git`.

## 1. The project file: `deps.fib`

```clojure
;; deps.fib
{:name "acme/app"
 :version "0.1.0"
 :paths ["src"]                                         ; default ["src"]
 :deps {acme/util  {:git/url "https://github.com/acme/util.git" :git/tag "v1.2.0"}
        acme/json  {:git/url "https://github.com/acme/json.git" :git/sha "3f1c…(40 hex)"}
        acme/http  {:git/url "git@github.com:acme/http.git" :git/tag "v0.3" :git/sha "9a0e…"}
        acme/local {:local/root "../local"}}
 :override {acme/base {:git/url "https://github.com/acme/base.git" :git/sha "…"}}
 :aliases {:test {:extra-paths ["specs"]}}
 :allow-urls ["http://intranet.example/x.git"]}
```

- **Data, read by the compiler's own reader** (`syntax.reader`), not evaluated. Keys are keywords; a library name is a symbol
  (`acme/util`, as deps.edn) or a string. Every key is optional. **An unknown key is an error** (deps.edn ignores them; a misspelt
  `:dpes` silently dropping every dependency is the kind of surprise Cargo's strictness avoids), and so is a key given twice.
- **Errors carry FILE:LINE:COL**: the reader's own messages for malformed text, and the position of the offending form for the rest
  (`deps.fib:2:2: unknown key :dpes in deps.fib (known: :name :version …)`, `deps.fib:2:13: :git/sha is a full 40-digit commit id`).
- **`:git/sha` is the full 40-hex id** (tools.deps accepts a prefix with a tag; a full id is what can be compared and locked without a
  network round trip). `:git/tag` alone is resolved once and locked; tag and sha together are both recorded (the sha wins, the tag is
  checked whenever the remote is asked).
- **`:paths` must be relative and stay inside the library** (no absolute path, no `..`): a library's roots are its own files.
- A library without a `deps.fib` is allowed: `:paths ["src"]`, no dependencies (tools.deps does the same for a bare repository).
- `:override` and `:allow-urls` are read from the **root** project only; a library's own are ignored (Cargo's `[patch]` is root-only for
  the same reason: the application decides).
- `:aliases`: the MVP reads `:test {:extra-paths [..]}` only (roots `fibc test` adds). `:extra-deps`, `:main-opts` and selecting an
  alias by flag (`-A:name`) are deferred (§8).

### 1.1 Why `deps.fib` and not TOML

The language already has a reader with positions; using it means no second parser in the compiler, the same literal syntax the user
writes every day, and comments that survive `fibc deps add` (§5.2). Clojure's ergonomics here *is* "the build file is data in the
language's own syntax".

### 1.2 Module names across libraries

A module `a.b` is the file `a/b.fib` under the first root that has it (spec/syntax.md §5). With several libraries on the search path,
two libraries that both have `util.fib` would make one of them silently unreachable. Clojure relies on reverse-domain namespaces by
convention; Cargo gives each crate its own namespace by construction.

**Decision: detection, not a enforced prefix.** At every resolution fibc lists every module of the project and of every library and
refuses a module name defined by two of them, or one that the bundled library defines:

    fibc: the module acme.util is defined twice: by acme/util (…/util/<sha>/src/acme/util.fib) and by acme/clash (…/src/acme/util.fib);
    a module name belongs to one library (name modules under a prefix of the library's own)

The convention (in the `fibc new` scaffold) is that library `acme/util` puts its modules under `acme.util` / `acme/util/`. Enforcing the
prefix was rejected for now: the bundled library's own modules (`fib.*`) and existing programs do not follow one, and an error at
resolution already makes the failure loud. Open question (§9).

## 2. The lockfile: `deps.lock`

```clojure
;; deps.lock: written by fibc from deps.fib (docs/design/packages.md). Commit it; do not edit it.
{:lock 1
 :deps {acme/util {:git/url "file:///…/util.git" :git/tag "v1"}}
 :override {}
 :libs [{:name "acme/util" :git/url "file:///…/util.git" :git/sha "44e0e7cd…" :git/tag "v1"
   :tree "8d0f…"
   :paths ["src"] :deps ["acme/base"] :via ["acme/app"]}
        {:name "acme/base" :git/url "file:///…/base.git" :git/sha "1cef735f…"
   :tree "3be2…"
   :paths ["src"] :deps [] :via ["acme/app" "acme/util"]}]}
```

- `:deps`/`:override` are the root's requests the lock was made from. The lock is **fresh** when they equal deps.fib's (order of the
  map does not matter); otherwise resolution runs again, pinned by the libraries the lock has (Cargo's behaviour: adding one dependency
  does not move the others).
- `:libs` is the resolved graph in **resolution order** (breadth first from the root in the order deps.fib writes them): the order of
  the module search path. Each has its commit, the commit's **tree id** (git's object id of the file tree: the integrity check, no
  separate checksum file, the hash git already computed), its roots, the names it depends on (the transitive graph) and `:via`, the chain
  that first asked for it (for messages and `deps tree`).
- A `:local/root` library is recorded by its path relative to the project; it has no commit.
- The lock is written only when resolution's result differs from it (byte-stable otherwise).

### 2.1 `--locked`, `--offline`, `--frozen`

As Cargo: `--locked` refuses a missing or stale lock, and a resolution whose result differs from the lock (an edited lock); it may still
fetch a locked commit that is not in the cache. `--offline` never runs a network git command: a library not in the cache is an error
that says so, and a tag not in the lock cannot be resolved. `--frozen` is both. They are taken by `build`, `run`, `test`, `emit`,
`explain` and `deps`, anywhere before `--`.

With a fresh lock and a warm cache **no build touches the network**, flags or not: tags are pinned, commits are in the cache. That is
tested by moving the remotes away (§7).

## 3. Resolution

Breadth first from the root's `:deps`:

1. A request's name in the root's `:override` is replaced by the override's coordinate (from every requester).
2. Its commit: the `:git/sha`; else the pinned commit (the lock's library of the same name, URL and tag, unless `deps update` names it);
   else the tag asked of the remote (`git ls-remote URL refs/tags/T refs/tags/T^{}`, an annotated tag peeled to its commit).
3. If the name is already resolved: the same URL and commit (or the same directory) is a diamond and fine. Anything else is a
   **CONFLICT** naming both chains:

       fibc: CONFLICT: acme/base is requested twice, at different commits:
         acme/c -> acme/util -> acme/base: file:///…/base.git 1cef735f… 
         acme/c -> acme/other -> acme/base: file:///…/base.git 9a1d… 
       fibc does not choose between commits; choose one under :override in the root deps.fib, e.g.
         :override {acme/base {:git/url ".." :git/sha ".."}}

   tools.deps picks the newest commit when it can order them; Cargo unifies semver-compatible versions. With bare commits there is no
   order that means "compatible", so a silent choice would build code nobody tested together. The owner's brief asked for no silent
   newest-wins; the conflict error with an explicit `:override` is that.
4. Otherwise the library is fetched (§4), verified, and its `deps.fib` read; its requests join the queue with `:via` extended.

A git library may not depend on a `:local/root` (the directory would be relative to a cache checkout; Cargo forbids path dependencies in
published crates for the same reason). A URL that is not `https://`, `ssh://`, `user@host:path` or `file://` is refused unless the root
lists it in `:allow-urls`.

**Moved tags.** A tagged library whose commit has to be fetched (not in the cache) has its tag asked of the remote first; `fibc deps
fetch` asks for every tag. A tag that no longer names the locked commit is refused:

    fibc deps: acme/tagged: the tag t1 of file:///…/tagged.git has moved: it named 7c9e… when locked and names 2b41… now;
    `fibc deps update acme/tagged` takes the new commit

A build with a warm cache keeps the locked commit and does not ask (no network). `fibc deps update [NAME..]` unpins NAME (all without
a name) and resolves its tag afresh.

## 4. The git cache

- `$FIBBER_HOME` (else `$XDG_CACHE_HOME/fibber`, else `~/.cache/fibber`) `/git/HOST/PATH/SHA`: `https://github.com/a/b.git` is
  `github.com/a/b`, `git@github.com:a/b.git` the same, `file:///x/r.git` is `file/x/r`; each part keeps `[A-Za-z0-9._-]` and maps the
  rest to `_` (`.` and `..` become `_`).
- A fetch: `git init` a fresh `SHA.tmp-PID`, `git fetch --depth 1 URL SHA` (exactly the commit; a server that refuses a fetch by id gets
  a full fetch of branches and tags), `git checkout --detach SHA`, verify, `chmod -R a-w`, then **rename** into `SHA`. rename(2) is
  atomic: two builds fetching the same commit at once both succeed (the loser's rename fails on the existing directory and it removes
  its own copy); a fetch that dies leaves only a `.tmp-PID` directory beside the checkouts, never a partial `SHA` (such leftovers stay until removed by
  hand; `gc` would take them). No lock files are needed.
  Tested with two concurrent `fibc deps fetch` into a cold cache.
- **Verification on every build**, for every git library: `git rev-parse HEAD HEAD^{tree}` must equal the locked commit and tree, and
  `git status --porcelain` must be empty (no changed, deleted or added file). A mismatch is an error naming the directory; removing it
  makes the next build fetch it again. Each costs two `git` processes per library per build (milliseconds).
- The checkouts are read-only; the cache is pure (every entry is determined by URL and commit), so deleting any part of it is always
  safe. A `fibc deps gc` (remove checkouts no project has used for N days, by a last-used stamp) is deferred: `rm -rf ~/.cache/fibber/git`
  is the MVP's garbage collector.

## 5. Commands

| Command | What |
|---|---|
| `fibc new NAME [--lib]` | `NAME/deps.fib`, `src/NAME/core.fib`, `specs/NAME-spec.fib`, `README.md`, `.gitignore`, and `src/main.fib` without `--lib`. NAME: a lower-case letter, then letters, digits, `-`. |
| `fibc build/run/test/emit/explain …` | **project mode** when a `deps.fib` is in the working directory or above (Cargo's rule): resolve (pinned by the lock), write the lock if it changed, check modules, then compile with the search path below. |
| `fibc deps tree` | the resolved graph, depth first; a library shown before is marked `(*)`. |
| `fibc deps fetch` | resolve, fetch everything, ask the remote for every tag (refuse a moved one), write the lock. |
| `fibc deps update [NAME..]` | resolve NAME's tag (all without NAME) afresh and write the lock. |
| `fibc deps add NAME --git URL --tag T \| --sha S` or `--local DIR` | edit deps.fib (below), resolve, lock; on failure deps.fib is restored. |
| `fibc deps path` | the module search path, one directory per line. |

**Search path in project mode:** the main file's directory, the `-I` directories, the project's `:paths` (and for `test` the `:test`
alias's `:extra-paths`), every library's `:paths` in lock order, then `FIB_LIB`, then the bundled library. A library therefore wins
over a module of the same name in `FIB_LIB` (tested), and the project's own modules come first (but a collision is refused anyway, §1.2).
`fibc test` resolves once and passes the directories to each spec's child as `-I` with `FIB_NO_PROJECT=1` so they do not resolve again.

Errors of project mode exit with status 2 for build/run/test (before anything is compiled), 1 for `fibc deps` (2 for its usage).

### 5.1 Why not a "fibc build" with no file

Cargo builds the package's targets with no argument. fibber programs are a main file today; `fibc build src/main.fib -o app` keeps the
command line the same in and out of a project. A `:main` key (and `fibc run` with no file) is cheap to add: open question (§9).

### 5.2 `deps add` keeps the user's text

The entry is inserted before the closing brace of `:deps` on a line of its own, indented as the first entry is (or `:deps {..}` is added
before the project map's closing brace). Positions come from the reader, so comments and layout survive (tested: a comment line and the
column of the first entry). Reprinting the map would be simpler and lose both.

## 6. Security: what a dependency can do

Say it plainly: **depending on a library means running its code on your machine at build time.** fibber macros are expanded by the
macro runner (`macros.runner`), which JIT-compiles each macro and executes it during `fibc build`, `run`, `test`, `emit` and `explain`
(spec/compiler.md §6). A macro is ordinary fibber code; through `extern` and `unsafe` it can call any C function the process can, so a
macro of a dependency can read and write your files and open sockets, exactly like a Rust proc-macro or `build.rs`. The library's
ordinary functions run when your program runs, of course.

What the package manager itself guarantees:

- **Resolution runs only `git`** (and `chmod`, `rm`, `mkdir` on the cache), with argument vectors (execvp, no shell), standard input from
  `/dev/null` and `GIT_TERMINAL_PROMPT=0` (no prompt hangs). It reads `deps.fib` files as data. No library code runs while fetching,
  resolving or locking (`fibc deps fetch|tree|update|add|path` run no library code at all).
- **What builds is what was locked**: every checkout's commit and tree id are compared with the lock, and its files with the commit, on
  every build.
- Only `https://`, `ssh://`, scp-like and `file://` URLs are fetched unless the root project lists a URL in `:allow-urls`.
- Library `:paths` cannot leave the library (no absolute paths, no `..`).
- A library cannot shadow a module of another library or of the bundled library (§1.2): a malicious dependency cannot replace
  `fib.core` or your `acme.util` by adding a file.

Not guaranteed: that the code at a commit is benign (review it; pin by sha), and that a git hook installed in your *global* git
configuration does not run during checkout (it is your configuration).

## 7. Tests, the gate and agents

- `compiler/tests/deps/run.sh STAGE2` (76 checks, about a minute, no network): `fixtures.sh` makes local bare repositories in a temp
  directory (`git init --bare`, commits, tags, a tag moved with a forced push) reached by `file://` URLs, with a cache of the test's own
  (`FIBBER_HOME`). It covers: a transitive chain with a macro used across the library boundary; lock contents (commit, tag, tree);
  `deps tree` and `deps path`; the checkout is read-only; a library beats `FIB_LIB`; `--locked`/`--frozen` builds with the remotes moved
  away; `--offline` with a cold cache (error) and warm (builds); a stale lock under `--locked`; a tampered file and a moved HEAD in a
  checkout; a diamond at one commit; a conflict naming both chains; `:override` resolving it; a tag moved after locking (build keeps the
  lock, `deps fetch` refuses, a cold-cache build refuses, `deps update` takes it); a module collision; deps.fib errors at their
  positions; an `http://` URL; a `:local/root` library and one that would shadow `fib.core`; `deps add` keeping a comment and indentation and restoring deps.fib after a refused add; `fibc new` then
  `fibc test` and `run`; two concurrent fetches.
- `scripts/mutant-deps.sh` (FIBC=a builder) plants six faults, one at a time, in a copy of the tree, builds a stage 2 from it and runs
  run.sh, which must fail: lock ignored, conflict silently resolved, commit not verified, files not verified, `--offline` ignored, search
  path order reversed.
- **Joining the gate** (not done here: scripts/gate.sh belongs to another agent): add `compiler/tests/deps/run.sh "$F"` to the tool tests
  of the full gate (it needs `git` on PATH and writes only under `~/.cache/fibber-scratch/deps-test.*`). The mutant script is a review,
  not a gate (it builds six stage 2s).
- Agents: never point a test at a network URL; build fixtures with `fixtures.sh`; set `FIBBER_HOME` to a scratch directory.

## 8. Deferred

- **A registry** (crates.io / Clojars) and **version ranges** (semver resolution): git commits are the unit until a registry exists;
  a registry index could itself be a git repository of `deps.fib` fragments (as crates.io's index was).
- **Publishing** (`fibc publish`), signing.
- **Binary caches**: compiled artifacts per library. Today every build compiles every module from source (the module cache is the
  compiler's business, not the package manager's).
- **Workspaces**: several projects sharing one lock. A minimal version (a root `deps.fib` with `:workspace ["a" "b"]` whose members are
  `:local/root` libraries) is mostly `:local/root` already: members can depend on each other by path today. One shared lock is the
  missing part; not trivial enough to include now.
- `fibc deps vendor` (copy the locked checkouts to `vendor/` and resolve from there): cheap (copy + a `:vendor` switch in the cache
  lookup), deferred to keep the MVP small; `--offline` with a warm cache covers air-gapped builds on one machine.
- `fibc deps gc` (§4); `:deps/root` (a library in a subdirectory of a repository); `:extra-deps` in aliases and `-A:alias`;
  `:main` for `fibc run` with no file.

## 9. Open questions for the owner

1. **Enforce a module prefix per library?** (e.g. library `acme/util` may only define `acme.util.*`). Today: collisions are detected and
   refused, the prefix is a convention.
2. **`:main` and `fibc run`/`build` with no file** in a project (Cargo-style), or keep the file explicit?
3. **Network on `fibc build` when the lock is stale**: today a build re-resolves and rewrites deps.lock (Cargo's default). Prefer
   `--locked` as the default (tools.deps' "never change the lock without `-X:deps`")?
4. **Exit status**: project-mode failures use 2 (usage/unreadable input). A distinct status (6) for "dependencies" would let scripts
   tell them apart.
5. Should `fibc deps fetch` (or every build) verify the tag of a tag+sha coordinate on every run, at the cost of one `git ls-remote`?
