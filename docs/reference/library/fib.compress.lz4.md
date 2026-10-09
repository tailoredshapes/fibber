# fib.compress.lz4

Generated declaration inventory of [fib.compress.lz4](../../../lib/fib/compress/lz4.fib). Source links contain the full signatures and API comments; enum variants and protocol methods belong to their linked declaration.

| Name | Declaration | Defined in |
|---|---|---|
| `Lz4` | defstruct | [lib/fib/compress/lz4.fib:12](../../../lib/fib/compress/lz4.fib#L12) |
| `Lz4Compressor` | defstruct | [lib/fib/compress/lz4.fib:17](../../../lib/fib/compress/lz4.fib#L17) |
| `Lz4Decompressor` | defstruct | [lib/fib/compress/lz4.fib:18](../../../lib/fib/compress/lz4.fib#L18) |
| `Xxh` | defstruct | [lib/fib/compress/lz4/xxh32.fib:55](../../../lib/fib/compress/lz4/xxh32.fib#L55) |
| `avalanche` | defun | [lib/fib/compress/lz4/xxh32.fib:19](../../../lib/fib/compress/lz4/xxh32.fib#L19) |
| `block-compress` | defun | [lib/fib/compress/lz4/block.fib:20](../../../lib/fib/compress/lz4/block.fib#L20) |
| `block-compress-hc` | defun | [lib/fib/compress/lz4/block.fib:21](../../../lib/fib/compress/lz4/block.fib#L21) |
| `block-decompress` | defun | [lib/fib/compress/lz4/block.fib:46](../../../lib/fib/compress/lz4/block.fib#L46) |
| `block-decompress-into` | defun | [lib/fib/compress/lz4/block.fib:32](../../../lib/fib/compress/lz4/block.fib#L32) |
| `block-decompress-sized` | defun | [lib/fib/compress/lz4/block.fib:37](../../../lib/fib/compress/lz4/block.fib#L37) |
| `bytes-tail` | defun | [lib/fib/compress/lz4/xxh32.fib:22](../../../lib/fib/compress/lz4/xxh32.fib#L22) |
| `check-level` | defun | [lib/fib/compress/lz4.fib:28](../../../lib/fib/compress/lz4.fib#L28) |
| `compress-bound` | defun | [lib/fib/compress/lz4/block.fib:13](../../../lib/fib/compress/lz4/block.fib#L13) |
| `decode-error` | defun | [lib/fib/compress/lz4/block.fib:23](../../../lib/fib/compress/lz4/block.fib#L23) |
| `decode-to` | defun | [lib/fib/compress/lz4/block.fib:29](../../../lib/fib/compress/lz4/block.fib#L29) |
| `lane32` | defun | [lib/fib/compress/lz4/xxh32.fib:40](../../../lib/fib/compress/lz4/xxh32.fib#L40) |
| `lz4` | defun | [lib/fib/compress/lz4.fib:13](../../../lib/fib/compress/lz4.fib#L13) |
| `lz4-compress` | defun | [lib/fib/compress/lz4.fib:40](../../../lib/fib/compress/lz4.fib#L40) |
| `lz4-decompress` | defun | [lib/fib/compress/lz4.fib:49](../../../lib/fib/compress/lz4.fib#L49) |
| `m32` | defun | [lib/fib/compress/lz4/xxh32.fib:14](../../../lib/fib/compress/lz4/xxh32.fib#L14) |
| `magic-bytes` | defun | [lib/fib/compress/lz4.fib:15](../../../lib/fib/compress/lz4.fib#L15) |
| `mul` | defun | [lib/fib/compress/lz4/xxh32.fib:16](../../../lib/fib/compress/lz4/xxh32.fib#L16) |
| `p1` | def | [lib/fib/compress/lz4/xxh32.fib:8](../../../lib/fib/compress/lz4/xxh32.fib#L8) |
| `p2` | def | [lib/fib/compress/lz4/xxh32.fib:9](../../../lib/fib/compress/lz4/xxh32.fib#L9) |
| `p3` | def | [lib/fib/compress/lz4/xxh32.fib:10](../../../lib/fib/compress/lz4/xxh32.fib#L10) |
| `p4` | def | [lib/fib/compress/lz4/xxh32.fib:11](../../../lib/fib/compress/lz4/xxh32.fib#L11) |
| `p5` | def | [lib/fib/compress/lz4/xxh32.fib:12](../../../lib/fib/compress/lz4/xxh32.fib#L12) |
| `par-cutoff-compress` | def | [lib/fib/compress/lz4.fib:33](../../../lib/fib/compress/lz4.fib#L33) |
| `par-cutoff-decompress` | def | [lib/fib/compress/lz4.fib:34](../../../lib/fib/compress/lz4.fib#L34) |
| `ptr-at` | defun | [lib/fib/compress/lz4/xxh32.fib:51](../../../lib/fib/compress/lz4/xxh32.fib#L51) |
| `rotl` | defun | [lib/fib/compress/lz4/xxh32.fib:15](../../../lib/fib/compress/lz4/xxh32.fib#L15) |
| `round` | defun | [lib/fib/compress/lz4/xxh32.fib:17](../../../lib/fib/compress/lz4/xxh32.fib#L17) |
| `run-enc` | defun | [lib/fib/compress/lz4/block.fib:15](../../../lib/fib/compress/lz4/block.fib#L15) |
| `slack` | def | [lib/fib/compress/lz4/block.fib:11](../../../lib/fib/compress/lz4/block.fib#L11) |
| `tail` | defun | [lib/fib/compress/lz4/xxh32.fib:25](../../../lib/fib/compress/lz4/xxh32.fib#L25) |
| `vinit` | defun | [lib/fib/compress/lz4/xxh32.fib:38](../../../lib/fib/compress/lz4/xxh32.fib#L38) |
| `vround` | defun | [lib/fib/compress/lz4/xxh32.fib:32](../../../lib/fib/compress/lz4/xxh32.fib#L32) |
| `vs` | defun | [lib/fib/compress/lz4/xxh32.fib:31](../../../lib/fib/compress/lz4/xxh32.fib#L31) |
| `vstripes` | defun | [lib/fib/compress/lz4/xxh32.fib:36](../../../lib/fib/compress/lz4/xxh32.fib#L36) |
| `vsum` | defun | [lib/fib/compress/lz4/xxh32.fib:41](../../../lib/fib/compress/lz4/xxh32.fib#L41) |
| `workers-for` | defun | [lib/fib/compress/lz4.fib:35](../../../lib/fib/compress/lz4.fib#L35) |
| `xload` | defun | [lib/fib/compress/lz4/xxh32.fib:82](../../../lib/fib/compress/lz4/xxh32.fib#L82) |
| `xset` | defun | [lib/fib/compress/lz4/xxh32.fib:58](../../../lib/fib/compress/lz4/xxh32.fib#L58) |
| `xstore` | defun | [lib/fib/compress/lz4/xxh32.fib:84](../../../lib/fib/compress/lz4/xxh32.fib#L84) |
| `xw` | defun | [lib/fib/compress/lz4/xxh32.fib:57](../../../lib/fib/compress/lz4/xxh32.fib#L57) |
| `xxh-digest` | defun | [lib/fib/compress/lz4/xxh32.fib:89](../../../lib/fib/compress/lz4/xxh32.fib#L89) |
| `xxh-feed` | defun | [lib/fib/compress/lz4/xxh32.fib:69](../../../lib/fib/compress/lz4/xxh32.fib#L69) |
| `xxh-new` | defun | [lib/fib/compress/lz4/xxh32.fib:60](../../../lib/fib/compress/lz4/xxh32.fib#L60) |
| `xxh-stripe` | defun | [lib/fib/compress/lz4/xxh32.fib:64](../../../lib/fib/compress/lz4/xxh32.fib#L64) |
| `xxh-update!` | defun | [lib/fib/compress/lz4/xxh32.fib:87](../../../lib/fib/compress/lz4/xxh32.fib#L87) |
| `xxh32` | defun | [lib/fib/compress/lz4/xxh32.fib:52](../../../lib/fib/compress/lz4/xxh32.fib#L52) |
| `xxh32-ptr` | defun | [lib/fib/compress/lz4/xxh32.fib:44](../../../lib/fib/compress/lz4/xxh32.fib#L44) |
| `xxh32-range` | defun | [lib/fib/compress/lz4/xxh32.fib:50](../../../lib/fib/compress/lz4/xxh32.fib#L50) |
