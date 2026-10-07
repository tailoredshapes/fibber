"""The reference server of the differential harness (scripts/http-diff.sh): python3's http.server with the same routes as diff-server.fib.
Prints its port; closing stdin stops it."""
import http.server, sys, time, threading


def pattern(n):
    return bytes(33 + (i % 90) for i in range(n))


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def reply(self, status, body=b"", headers=()):
        self.send_response(status)
        for k, v in headers:
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def body(self):
        if self.headers.get("Transfer-Encoding", "").lower() == "chunked":
            out = b""
            while True:
                size = int(self.rfile.readline().split(b";")[0], 16)
                if size == 0:
                    while self.rfile.readline() not in (b"\r\n", b""):
                        pass
                    return out
                out += self.rfile.read(size)
                self.rfile.readline()
        return self.rfile.read(int(self.headers.get("Content-Length", 0)))

    def route(self):
        u = self.path
        data = self.body() if self.command in ("POST", "PUT") else b""
        if u == "/hello":
            self.reply(200, b"hello\n", [("Content-Type", "text/plain; charset=utf-8"), ("X-Route", "hello")])
        elif u == "/echo":
            self.reply(200, data, [("Content-Type", "application/octet-stream")])
        elif u == "/chunked":
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            for i in range(1, 5):
                piece = f"chunk-{i}\n".encode()
                self.wfile.write(f"{len(piece):x}\r\n".encode() + piece + b"\r\n")
            self.wfile.write(b"0\r\n\r\n")
        elif u in ("/redirect", "/redirect301"):
            self.reply(302 if u == "/redirect" else 301, b"", [("Location", "/hello")])
        elif u == "/big":
            self.reply(200, pattern(1048576), [("Content-Type", "application/octet-stream")])
        elif u == "/probe":
            self.reply(200, self.headers.get("X-Probe", "none").encode(), [("Content-Type", "text/plain; charset=utf-8")])
        elif u == "/slow":
            time.sleep(0.3)
            self.reply(200, b"slow\n", [("Content-Type", "text/plain; charset=utf-8")])
        elif u.startswith("/status/"):
            self.reply(int(u[8:]), b"status\n", [("Content-Type", "text/plain; charset=utf-8")])
        else:
            self.reply(404, b"not found\n", [("Content-Type", "text/plain; charset=utf-8")])

    do_GET = do_POST = do_PUT = do_HEAD = do_DELETE = route


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
server.daemon_threads = True
print(server.server_port, flush=True)
threading.Thread(target=server.serve_forever, daemon=True).start()
sys.stdin.read()
server.shutdown()
