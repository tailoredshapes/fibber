# fib.http

Generated declaration inventory of [fib.http](../../../lib/fib/http.fib). Source links contain the full signatures and API comments; enum variants and protocol methods belong to their linked declaration.

| Name | Declaration | Defined in |
|---|---|---|
| `ClientRequest` | defstruct | [lib/fib/http/types.fib:22](../../../lib/fib/http/types.fib#L22) |
| `Error` | defenum | [lib/fib/http/types.fib:13](../../../lib/fib/http/types.fib#L13) |
| `Request` | defstruct | [lib/fib/http/types.fib:19](../../../lib/fib/http/types.fib#L19) |
| `Response` | defstruct | [lib/fib/http/types.fib:3](../../../lib/fib/http/types.fib#L3) |
| `ascii-lower` | defun | [lib/fib/http/headers.fib:3](../../../lib/fib/http/headers.fib#L3) |
| `base64-encode` | defun | [lib/fib/http/url.fib:48](../../../lib/fib/http/url.fib#L48) |
| `body-text` | defun | [lib/fib/http/bytes.fib:20](../../../lib/fib/http/bytes.fib#L20) |
| `concat-bytes` | defun | [lib/fib/http/bytes.fib:26](../../../lib/fib/http/bytes.fib#L26) |
| `decode-text` | defun | [lib/fib/http/bytes.fib:18](../../../lib/fib/http/bytes.fib#L18) |
| `error-message` | defun | [lib/fib/http/types.fib:23](../../../lib/fib/http/types.fib#L23) |
| `form-body` | defun | [lib/fib/http/url.fib:23](../../../lib/fib/http/url.fib#L23) |
| `form-decode` | defun | [lib/fib/http/url.fib:28](../../../lib/fib/http/url.fib#L28) |
| `form-encode` | defun | [lib/fib/http/url.fib:3](../../../lib/fib/http/url.fib#L3) |
| `header` | defun | [lib/fib/http/headers.fib:29](../../../lib/fib/http/headers.fib#L29) |
| `header-add` | defun | [lib/fib/http/headers.fib:33](../../../lib/fib/http/headers.fib#L33) |
| `header-set` | defun | [lib/fib/http/headers.fib:31](../../../lib/fib/http/headers.fib#L31) |
| `header-token?` | defun | [lib/fib/http/headers.fib:49](../../../lib/fib/http/headers.fib#L49) |
| `header-value?` | defun | [lib/fib/http/headers.fib:18](../../../lib/fib/http/headers.fib#L18) |
| `header-values` | defun | [lib/fib/http/headers.fib:27](../../../lib/fib/http/headers.fib#L27) |
| `latin1-text` | defun | [lib/fib/http/bytes.fib:37](../../../lib/fib/http/bytes.fib#L37) |
| `normalize-headers` | defun | [lib/fib/http/headers.fib:23](../../../lib/fib/http/headers.fib#L23) |
| `parse-query` | defun | [lib/fib/http/url.fib:39](../../../lib/fib/http/url.fib#L39) |
| `query-string` | defun | [lib/fib/http/url.fib:15](../../../lib/fib/http/url.fib#L15) |
| `response` | defun | [lib/fib/http/bytes.fib:23](../../../lib/fib/http/bytes.fib#L23) |
| `text-response` | defun | [lib/fib/http/bytes.fib:21](../../../lib/fib/http/bytes.fib#L21) |
| `token?` | defun | [lib/fib/http/headers.fib:9](../../../lib/fib/http/headers.fib#L9) |
| `trim-ows` | defun | [lib/fib/http/headers.fib:44](../../../lib/fib/http/headers.fib#L44) |
| `utf8?` | defun | [lib/fib/http/bytes.fib:3](../../../lib/fib/http/bytes.fib#L3) |
| `validate-headers` | defun | [lib/fib/http/headers.fib:36](../../../lib/fib/http/headers.fib#L36) |
| `with-header` | defun | [lib/fib/http/bytes.fib:24](../../../lib/fib/http/bytes.fib#L24) |
| `with-query` | defun | [lib/fib/http/url.fib:19](../../../lib/fib/http/url.fib#L19) |
