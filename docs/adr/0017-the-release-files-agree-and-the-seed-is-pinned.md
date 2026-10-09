# 0017. The release files agree and the seed is pinned

Status: accepted
Date: 2026-10-06
Source: CLAUDE.md (`scripts/`: `check-version.sh`, `fetch-seed.sh`, "`SEED` at the root names the release (url, sha256) that builds stage 2"); ROADMAP.md, Releases;
scripts/check-version.sh; .github/workflows/release.yml.

## Context

A release has three facts written in three places: the file `VERSION` (the release line), the constant `version` of
`compiler/driver/version.fib` (a fibber program cannot read a file at build time, so the constant repeats it) and the tag. The seed that
builds stage 2 is named in `SEED` by url and sha256, one pair per platform. `scripts/check-version.sh` compared the first two, but only
`scripts/package.sh` ran it, so a commit could break the agreement and nothing but a release would notice. A seed whose url names one
version and whose file another, or whose checksum is not a checksum, fails at the first build on a clean machine.

## Decision

1. `VERSION` and the constant of `compiler/driver/version.fib` are the same text. (The rule of `scripts/check-version.sh`, now in the gate.)
2. Every platform of `SEED` has a url and a sha256, and no sha256 is without a url. A sha256 is 64 lower-case hex digits.
3. A seed url has the form `https://github.com/tailoredshapes/fibber/releases/download/vX.Y.Z/fibc-X.Y.Z-PLATFORM.tar.gz`: the tag and the
   file name carry the same version and the file name carries the platform of its key.
4. The seed is not newer than the tree: its version is at most `VERSION`.
5. The release path keeps its checks: `scripts/package.sh` runs `scripts/check-version.sh`, and the release workflow refuses a tag that
   is not `v` plus `VERSION`.

## Consequences

- A `VERSION` bump that forgets `version.fib`, or the reverse, fails `make adr` in the full gate, not at the tag.
- A new platform's seed is added as a url and a checksum together, in the form above.
- `SEED` names a release that exists *when its sha256 matches the download*: `scripts/fetch-seed.sh` checks that and the gate's build uses it.
  Whether a url is *published* is not checked offline (see below).

## Governance

```fibber fitness
(defstruct KV (key: str val: str))

(defun seed-kvs (repo: Repo) -> (Vec KV)
  (reduce (fn (acc: (Vec KV) n: NumLine)
            (if (or (starts-with? (. n text) "#") (= (. n text) ""))
                acc
                (match (str-find (. n text) "=" 0)
                  (nil acc)
                  ((some k) (conj acc (KV (str-slice (. n text) 0 k) (str-slice (. n text) (+ k 1) (str-len (. n text)))))))))
          [] (numbered (unwrap-or (file-text repo "SEED") ""))))

(defun plat-of (key: str prefix: str) -> (Option str)
  (cond (= key prefix) (some "linux-x86_64")
        (starts-with? key (str prefix ".")) (some (str-slice key (+ (str-len prefix) 1) (str-len key)))
        :else nil))

;; The (platform, value) rows of the lines whose key is `prefix` or `prefix.PLATFORM`.
(defun rows-of (repo: Repo prefix: str) -> (Vec KV)
  (reduce (fn (acc: (Vec KV) kv: KV) (match (plat-of (. kv key) prefix) (nil acc) ((some p) (conj acc (KV p (. kv val))))))
          [] (seed-kvs repo)))

(defun hex-digit? (b: i64) -> bool (or (and (>= b 48) (<= b 57)) (and (>= b 97) (<= b 102))))

(defun hex64? (s: str) -> bool
  (and (= (str-len s) 64)
       (loop ((i 0)) (cond (>= i 64) true (hex-digit? (sext i64 (str-byte-at s i))) (recur (+ i 1)) :else false))))

(defun version-of (repo: Repo) -> str (trim (unwrap-or (file-text repo "VERSION") "")))

(defun parts-of (v: str) -> (Vec i64)
  (loop ((i 0) (out []))
    (match (str-find v "." i)
      (nil (conj out (unwrap-or (parse-long (str-slice v i (str-len v))) -1)))
      ((some k) (recur (+ k 1) (conj out (unwrap-or (parse-long (str-slice v i k)) -1)))))))

(defun newer? (a: str b: str) -> bool
  (let ((x (parts-of a)) (y (parts-of b)))
    (loop ((i 0))
      (cond (or (>= i (count x)) (>= i (count y))) false
            (> (nth x i) (nth y i)) true
            (< (nth x i) (nth y i)) false
            :else (recur (+ i 1))))))

(defun url-prefix () -> str "https://github.com/tailoredshapes/fibber/releases/download/v")

;; "" when the url has the form for the platform, else what is wrong.
(defun url-fault (url: str plat: str) -> str
  (if (not (starts-with? url (url-prefix)))
      "does not start with the project's release url"
      (let ((rest (str-slice url (str-len (url-prefix)) (str-len url))))
        (match (str-find rest "/" 0)
          (nil "has no file name")
          ((some k) (let ((ver (str-slice rest 0 k)))
                      (if (= (str-slice rest k (str-len rest)) (str "/fibc-" ver "-" plat ".tar.gz")) ""
                          (str "file name is not fibc-" ver "-" plat ".tar.gz")))))))) 

(defun seed-version (repo: Repo) -> str
  (let ((rows (rows-of repo "url")))
    (if (empty? rows) "" (let ((u (. (nth rows 0) val))) (let ((rest (str-slice u (min (str-len (url-prefix)) (str-len u)) (str-len u))))
      (match (str-find rest "/" 0) (nil "") ((some k) (str-slice rest 0 k))))))))

(rule "VERSION and compiler/driver/version.fib say the same version"
  (must-contain repo "compiler/driver/version.fib" (str "(def version: str \"" (version-of repo) "\")"))
  (plant-file "VERSION" "9.9.9\n")
  (plant-file "compiler/driver/version.fib" "(ns driver.version)\n(def version: str \"0.0.1\")\n"))

(rule "every platform of SEED has a url and a sha256 of 64 lower-case hex digits, and no sha256 is without a url"
  (into (mapv (fn (u: KV) (Finding "SEED" 0 (str (. u key) ": no sha256 for this url")))
              (filterv (fn (u: KV) (empty? (filterv (fn (s: KV) (= (. s key) (. u key))) (rows-of repo "sha256")))) (rows-of repo "url")))
        (into (mapv (fn (s: KV) (Finding "SEED" 0 (str (. s key) ": a sha256 without a url")))
                    (filterv (fn (s: KV) (empty? (filterv (fn (u: KV) (= (. s key) (. u key))) (rows-of repo "url")))) (rows-of repo "sha256")))
              (mapv (fn (s: KV) (Finding "SEED" 0 (str (. s key) ": the sha256 is not 64 lower-case hex digits")))
                    (filterv (fn (s: KV) (not (hex64? (. s val)))) (rows-of repo "sha256")))))
  (plant-file "SEED" "url=https://github.com/tailoredshapes/fibber/releases/download/v0.1.9/fibc-0.1.9-linux-x86_64.tar.gz\nsha256=abc\n")
  (plant-file "SEED" "url=https://github.com/tailoredshapes/fibber/releases/download/v0.1.9/fibc-0.1.9-linux-x86_64.tar.gz\n"))

(rule "every seed url has the form of the project's releases, with one version and the platform of its key"
  (mapv (fn (u: KV) (Finding "SEED" 0 (str (. u key) ": the url " (url-fault (. u val) (. u key)))))
        (filterv (fn (u: KV) (not (= (url-fault (. u val) (. u key)) ""))) (rows-of repo "url")))
  (plant-file "SEED" "url=https://github.com/tailoredshapes/fibber/releases/download/v0.1.9/fibc-0.1.8-linux-x86_64.tar.gz\n")
  (plant-file "SEED" "url=https://example.com/fibc.tar.gz\n"))

(rule "SEED has at least the linux-x86_64 row, and the seed is not newer than VERSION"
  (into (if (empty? (filterv (fn (u: KV) (= (. u key) "linux-x86_64")) (rows-of repo "url"))) [(Finding "SEED" 0 "no linux-x86_64 url")] [])
        (if (newer? (seed-version repo) (version-of repo)) [(Finding "SEED" 0 (str "the seed " (seed-version repo) " is newer than VERSION " (version-of repo)))] []))
  (plant-file "SEED" "url=https://github.com/tailoredshapes/fibber/releases/download/v9.0.0/fibc-9.0.0-linux-x86_64.tar.gz\nsha256=4842cb0700ad3147b4f1543737145645571c9e397c3dc027b104cd46d708d06b\n")
  (plant-file "SEED" "# empty\n"))

(rule "the release path keeps its checks: the release depends on build/version.ok, whose recipe is check-version.sh; the workflow refuses a tag that is not v + VERSION"
  (into (must-contain repo "mk/stage2.mk" "scripts/check-version.sh")
        (into (must-contain repo "mk/release.mk" "$(BUILD)/version.ok")
              (into (must-contain repo ".github/workflows/release.yml" "test \"v$(cat VERSION)\" = \"$TAG\"")
                    (missing repo ["scripts/check-version.sh" "scripts/fetch-seed.sh" "scripts/package.sh"]))))
  (plant-file "mk/release.mk" "release:\n")
  (plant-remove "scripts/check-version.sh"))
```

### What this does not check

That the url is **published** (it needs the network; `scripts/fetch-seed.sh` downloads and compares the sha256 when a build needs the seed, and the
gate's stage-2 build runs it); that the sha256 is the right one (the same script); that the tag exists in git; that `VERSION` was
bumped when it should have been (a release decision, ROADMAP.md, Releases).
