# 0020. A script that downloads also checks a checksum

Status: accepted
Date: 2026-10-06
Source: memory ("Tool downloads OK": owner authorised fetching tools; scratch only, verified, recorded); scripts/fetch-seed.sh; scripts/fetch-json-testsuite.sh;
docs/design/package-manager.md (tags and hashes).

## Context

The owner authorised fetching tools (a seed compiler, test suites, platform toolchains) on three conditions: into scratch, verified, and
recorded. "Verified" is a checksum compared with one that is written in the repository, so that a changed or substituted download fails
instead of running. Two scripts do it (`scripts/fetch-seed.sh` against `SEED`, `scripts/fetch-json-testsuite.sh` against
`specs/json-testsuite.sha256`). The condition was prose, so a third script could download and trust.

## Decision

A shell script of `scripts/`, of `compiler/tests/` or of the workflows of `.github/workflows/`, and since MAKE-1 the `Makefile` and
`mk/*.mk` (a recipe line, whatever its leading tab, `@`, `-` or `+`, is a live line), that downloads (a live line that starts with
`curl`, `wget`, `git clone` or `gh release download`) also verifies what it got in the same file: a live line with `sha256sum`,
`shasum` or `dgst -sha256`. The exceptions are listed below with the reason; the list only shrinks.

The two CI workflows fetch LLVM's installer (`wget -qO llvm.sh https://apt.llvm.org/llvm.sh`) without a checksum: it is the LLVM project's
own installer for apt, run on a CI runner only, and it fetches signed packages. The way out is a pinned apt source with the project's
key; until then those two lines are listed.

## Consequences

- A new fetch script cannot be added without a checksum check, or an entry here that a reviewer sees.
- `scripts/fetch-seed.sh` and `scripts/fetch-json-testsuite.sh` pass because they check; they stay the model for a new one.
- The *scratch only* condition is not checked (see below).

## Governance

```fibber fitness
(defun download-line? (t: str) -> bool
  (or (or (starts-with? t "curl ") (starts-with? t "wget ")) (or (starts-with? t "git clone ") (starts-with? t "gh release download"))))

(defun check-line? (t: str) -> bool
  (or (or (some? (str-find t "sha256sum" 0)) (some? (str-find t "shasum" 0))) (some? (str-find t "dgst -sha256" 0))))

(defun live? (t: str) -> bool (not (starts-with? t "#")))

;; A recipe line of a Makefile: the leading tab and the `@`, `-`, `+` prefixes are not the command.
(defun recipe-text (t: str) -> str
  (let ((u (trim t)))
    (if (and (> (str-len u) 0) (or (starts-with? u "@") (or (starts-with? u "-") (starts-with? u "+")))) (trim (str-slice u 1 (str-len u))) u)))

(defun first-word-of (t: str) -> str (match (str-find t " " 0) (nil t) ((some k) (str-slice t 0 k))))

;; One finding for each file that has a download line and no checksum line: the file, the line of the first download, the tool.
(defun unchecked-downloads (repo: Repo) -> (Vec Finding)
  (reduce (fn (acc: (Vec Finding) f: SrcFile)
            (let ((ls (filterv (fn (n: NumLine) (live? (recipe-text (. n text)))) (numbered (. f text)))))
              (let ((ds (filterv (fn (n: NumLine) (download-line? (recipe-text (. n text)))) ls)))
                (if (or (empty? ds) (not (empty? (filterv (fn (n: NumLine) (check-line? (. n text))) ls))))
                    acc
                    (conj acc (Finding (. f path) (. (nth ds 0) n) (str (first-word-of (. (nth ds 0) text)) " downloads, and the file checks no checksum")))))))
          [] (select repo ["scripts/**.sh" "compiler/tests/**.sh" ".github/workflows/*.yml" "Makefile" "mk/*.mk"])))

(rule "every script, workflow and Makefile that downloads also checks a sha256, except the two LLVM installer lines"
  (allowing (unchecked-downloads repo) [".github/workflows/ci.yml:wget" ".github/workflows/release.yml:wget"])
  (plant-file "scripts/zz-fetch.sh" "#!/bin/sh\ncurl -fsSL -o /tmp/x https://example.com/x.tar.gz\ntar xzf /tmp/x\n")
  (plant "scripts/gate.sh" "\ngit clone https://example.com/tool\n")
  (plant-file "mk/zz-fetch.mk" "x:\n\t@curl -fsSL -o /tmp/x https://example.com/x.tar.gz\n")
  (plant "mk/fetch.mk" "\ny:\n\tgit clone https://example.com/tool\n"))

(rule "the two fetch scripts that exist do check: the seed's and the JSON test suite's"
  (into (must-contain repo "scripts/fetch-seed.sh" "sha256sum")
        (must-contain repo "scripts/fetch-json-testsuite.sh" "sha256sum -c"))
  (plant-file "scripts/fetch-seed.sh" "#!/bin/sh\ncurl -o x y\n")
  (plant-remove "scripts/fetch-json-testsuite.sh"))
```

### What this does not check

That the checksum is compared with the downloaded file (a file that merely mentions `sha256sum` passes: the reader does not run
the script; `scripts/tests/fetch-seed.sh` does, for the seed); that the checksum is *recorded* in the repository rather than fetched from
the same server; downloads made by a program other than these four tools (a `python3 urlretrieve`, the package manager's `deps fetch`, which
has its own hash check: compiler/tests/deps); **scratch only** (that the destination is under `~/.cache/fibber-scratch`: scripts take it
as an argument or default to it, and judging a path is review).
