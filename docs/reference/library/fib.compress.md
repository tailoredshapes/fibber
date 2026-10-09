# fib.compress

Generated declaration inventory of [fib.compress](../../../lib/fib/compress.fib). Source links contain the full signatures and API comments; enum variants and protocol methods belong to their linked declaration.

| Name | Declaration | Defined in |
|---|---|---|
| `Codec` | defprotocol | [lib/fib/compress/types.fib:87](../../../lib/fib/compress/types.fib#L87) |
| `CompressError` | defstruct | [lib/fib/compress/types.fib:16](../../../lib/fib/compress/types.fib#L16) |
| `Compressor` | defprotocol | [lib/fib/compress/types.fib:72](../../../lib/fib/compress/types.fib#L72) |
| `DecStep` | defstruct | [lib/fib/compress/types.fib:68](../../../lib/fib/compress/types.fib#L68) |
| `Decompressor` | defprotocol | [lib/fib/compress/types.fib:79](../../../lib/fib/compress/types.fib#L79) |
| `Options` | defstruct | [lib/fib/compress/types.fib:49](../../../lib/fib/compress/types.fib#L49) |
| `check-options` | defun | [lib/fib/compress/ops.fib:14](../../../lib/fib/compress/ops.fib#L14) |
| `checksum-error` | defun | [lib/fib/compress/types.fib:25](../../../lib/fib/compress/types.fib#L25) |
| `chunks-reader` | defun | [lib/fib/compress/ops.fib:62](../../../lib/fib/compress/ops.fib#L62) |
| `compress` | defun | [lib/fib/compress/ops.fib:22](../../../lib/fib/compress/ops.fib#L22) |
| `compress-chunks` | defun | [lib/fib/compress/ops.fib:80](../../../lib/fib/compress/ops.fib#L80) |
| `compress-default` | defun | [lib/fib/compress/ops.fib:28](../../../lib/fib/compress/ops.fib#L28) |
| `compress-stream` | defun | [lib/fib/compress/ops.fib:37](../../../lib/fib/compress/ops.fib#L37) |
| `concat-bytes` | defun | [lib/fib/compress/ops.fib:66](../../../lib/fib/compress/ops.fib#L66) |
| `corrupt` | defun | [lib/fib/compress/types.fib:20](../../../lib/fib/compress/types.fib#L20) |
| `decompress` | defun | [lib/fib/compress/ops.fib:25](../../../lib/fib/compress/ops.fib#L25) |
| `decompress-auto` | defun | [lib/fib/compress/ops.fib:105](../../../lib/fib/compress/ops.fib#L105) |
| `decompress-chunks` | defun | [lib/fib/compress/ops.fib:85](../../../lib/fib/compress/ops.fib#L85) |
| `decompress-default` | defun | [lib/fib/compress/ops.fib:29](../../../lib/fib/compress/ops.fib#L29) |
| `decompress-stream` | defun | [lib/fib/compress/ops.fib:53](../../../lib/fib/compress/ops.fib#L53) |
| `default-max-output` | def | [lib/fib/compress/types.fib:31](../../../lib/fib/compress/types.fib#L31) |
| `default-options` | defun | [lib/fib/compress/types.fib:51](../../../lib/fib/compress/types.fib#L51) |
| `detect` | defun | [lib/fib/compress/ops.fib:101](../../../lib/fib/compress/ops.fib#L101) |
| `drain` | defun | [lib/fib/compress/ops.fib:46](../../../lib/fib/compress/ops.fib#L46) |
| `error-kind` | defun | [lib/fib/compress/types.fib:28](../../../lib/fib/compress/types.fib#L28) |
| `internal-error` | defun | [lib/fib/compress/types.fib:27](../../../lib/fib/compress/types.fib#L27) |
| `invalid-argument` | defun | [lib/fib/compress/types.fib:24](../../../lib/fib/compress/types.fib#L24) |
| `io-error` | defun | [lib/fib/compress/types.fib:26](../../../lib/fib/compress/types.fib#L26) |
| `lookup` | defun | [lib/fib/compress/ops.fib:94](../../../lib/fib/compress/ops.fib#L94) |
| `max-compressed-size` | defun | [lib/fib/compress/ops.fib:8](../../../lib/fib/compress/ops.fib#L8) |
| `name-of` | defun | [lib/fib/compress/ops.fib:7](../../../lib/fib/compress/ops.fib#L7) |
| `need` | defun | [lib/fib/compress/ops.fib:10](../../../lib/fib/compress/ops.fib#L10) |
| `registry` | defun | [lib/fib/compress/ops.fib:91](../../../lib/fib/compress/ops.fib#L91) |
| `split-sizes` | defun | [lib/fib/compress/ops.fib:72](../../../lib/fib/compress/ops.fib#L72) |
| `starts-with-bytes?` | defun | [lib/fib/compress/ops.fib:96](../../../lib/fib/compress/ops.fib#L96) |
| `supports?` | defun | [lib/fib/compress/ops.fib:6](../../../lib/fib/compress/ops.fib#L6) |
| `too-large` | defun | [lib/fib/compress/types.fib:22](../../../lib/fib/compress/types.fib#L22) |
| `truncated` | defun | [lib/fib/compress/types.fib:21](../../../lib/fib/compress/types.fib#L21) |
| `unsupported` | defun | [lib/fib/compress/types.fib:23](../../../lib/fib/compress/types.fib#L23) |
| `with-acceleration` | defun | [lib/fib/compress/types.fib:64](../../../lib/fib/compress/types.fib#L64) |
| `with-block-checksum` | defun | [lib/fib/compress/types.fib:57](../../../lib/fib/compress/types.fib#L57) |
| `with-block-size` | defun | [lib/fib/compress/types.fib:58](../../../lib/fib/compress/types.fib#L58) |
| `with-checksum` | defun | [lib/fib/compress/types.fib:56](../../../lib/fib/compress/types.fib#L56) |
| `with-dictionary` | defun | [lib/fib/compress/types.fib:55](../../../lib/fib/compress/types.fib#L55) |
| `with-independent` | defun | [lib/fib/compress/types.fib:59](../../../lib/fib/compress/types.fib#L59) |
| `with-level` | defun | [lib/fib/compress/types.fib:53](../../../lib/fib/compress/types.fib#L53) |
| `with-max-output` | defun | [lib/fib/compress/types.fib:54](../../../lib/fib/compress/types.fib#L54) |
| `with-record-size` | defun | [lib/fib/compress/types.fib:60](../../../lib/fib/compress/types.fib#L60) |
| `with-size-hint` | defun | [lib/fib/compress/types.fib:61](../../../lib/fib/compress/types.fib#L61) |
| `with-threads` | defun | [lib/fib/compress/types.fib:62](../../../lib/fib/compress/types.fib#L62) |
| `with-verify` | defun | [lib/fib/compress/types.fib:63](../../../lib/fib/compress/types.fib#L63) |
| `write-nonempty` | defun | [lib/fib/compress/ops.fib:33](../../../lib/fib/compress/ops.fib#L33) |
