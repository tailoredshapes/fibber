"""The reference server of scripts/bench/http.py: python3's http.server, HTTP/1.1 keep-alive, /small (100 B) and /big (1 MiB). Prints its port; stops when stdin closes."""
import http.server, sys, threading

SMALL, BIG = b"x" * 100, b"y" * 1048576


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def do_GET(self):
        body = BIG if self.path.startswith("/big") else SMALL
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class S(http.server.ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 256


srv = S(("127.0.0.1", 0), H)
print(srv.server_port, flush=True)
threading.Thread(target=srv.serve_forever, daemon=True).start()
sys.stdin.read()
srv.shutdown()
