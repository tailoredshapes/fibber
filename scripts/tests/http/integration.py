#!/usr/bin/env python3
"""Independent, offline HTTP interoperability and failure-path checks."""
import argparse
import concurrent.futures
import gzip
import email.utils
import http.client
import http.server
import os
from pathlib import Path
import selectors
import socket
import ssl
import subprocess
import tempfile
import threading
import time
import unittest
from ownership import audit_owned_objects

ARGS = None
BINARY = b"\x00\xff\x80A"


def fields(output):
    result = {}
    for line in output.splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            result.setdefault(key, []).append(value)
    return result


def client(url, trace=False, **options):
    env = {k: v for k, v in os.environ.items() if not k.startswith("FIB_HTTP_")}
    env["FIB_HTTP_URL"] = url
    env.pop("FIB_TRACE", None)
    if trace:
        env["FIB_TRACE"] = "1"
    env.update({f"FIB_HTTP_{key.upper()}": str(value) for key, value in options.items()})
    return subprocess.run([ARGS.client], env=env, capture_output=True, text=True, timeout=10)


class FibberServer:
    def __init__(self, trace=False):
        self.errors = tempfile.TemporaryFile(mode="w+")
        env = dict(os.environ)
        env.pop("FIB_TRACE", None)
        if trace:
            env["FIB_TRACE"] = "1"
        self.process = subprocess.Popen(
            [ARGS.server], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.errors, text=True, env=env,
        )
        selector = selectors.DefaultSelector()
        selector.register(self.process.stdout, selectors.EVENT_READ)
        try:
            if not selector.select(10):
                raise AssertionError("Fibber server did not report its port")
            line = self.process.stdout.readline().strip()
            if not line.isdecimal():
                self.errors.seek(0)
                raise AssertionError(f"Server startup failed: {line!r} {self.errors.read()}")
            self.port = int(line)
        except BaseException:
            self.process.kill()
            self.process.communicate(timeout=10)
            self.errors.close()
            raise
        finally:
            selector.close()

    def close(self):
        try:
            out, _ = self.process.communicate("x", timeout=5)
            self.errors.seek(0)
            errors = self.errors.read()
            if self.process.returncode != 0:
                raise AssertionError(f"Server shutdown failed: {out!r} {errors}")
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.communicate(timeout=5)
            self.errors.close()
        return errors

    def request(self, method, path, body=None, **kwargs):
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=3)
        try:
            connection.request(method, path, body=body, **kwargs)
            response = connection.getresponse()
            return response.status, response.getheaders(), response.read()
        finally:
            connection.close()


def raw_response(stream, method="GET"):
    line = stream.readline()
    if not line:
        raise AssertionError("Server closed before sending a status line")
    status = int(line.split(b" ", 2)[1])
    headers = {}
    while True:
        line = stream.readline()
        if line == b"\r\n":
            break
        if not line:
            raise AssertionError("Server closed within response headers")
        key, value = line.rstrip(b"\r\n").split(b":", 1)
        headers.setdefault(key.lower(), []).append(value.strip())
    length = int(headers.get(b"content-length", [b"0"])[0])
    body = b"" if method == "HEAD" or status in (204, 304) else stream.read(length)
    return status, headers, body


class ServerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = FibberServer()

    @classmethod
    def tearDownClass(cls):
        cls.server.close()

    def raw(self, data, shutdown=False):
        with socket.create_connection(("127.0.0.1", self.server.port), 3) as sock:
            sock.sendall(data)
            if shutdown:
                sock.shutdown(socket.SHUT_WR)
            with sock.makefile("rb") as stream:
                return raw_response(stream)

    def test_binary_echo_and_bound_port(self):
        self.assertEqual(self.server.request("POST", "/echo", BINARY)[::2], (200, BINARY))
        self.assertEqual(self.server.request("GET", "/port")[2], str(self.server.port).encode())

    def test_query_and_repeated_headers(self):
        self.assertEqual(self.server.request("GET", "/query?a=1&a=2")[2], b"a=1&a=2")
        status, headers, body = self.server.request("GET", "/cookies")
        self.assertEqual(status, 200)
        self.assertEqual([v for k, v in headers if k.lower() == "set-cookie"], ["a=1", "b=2"])
        self.assertIn(("x-server", "fibber"), headers)
        self.assertEqual(email.utils.parsedate_to_datetime(dict(headers)["date"]).utcoffset().total_seconds(), 0)

    def test_fragmented_request_and_pipelining(self):
        with socket.create_connection(("127.0.0.1", self.server.port), 3) as sock:
            for part in [b"PO", b"ST /echo HTTP/1.1\r", b"\nHost: x\r\nContent-Length: 4\r\n\r\n", BINARY[:1], BINARY[1:]]:
                sock.sendall(part)
            sock.sendall(b"GET /health HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
            with sock.makefile("rb") as stream:
                self.assertEqual(raw_response(stream)[::2], (200, BINARY))
                self.assertEqual(raw_response(stream)[::2], (200, b"ok"))
                self.assertEqual(stream.read(1), b"")

    def test_chunked_request_and_trailers(self):
        result = self.raw(b"POST /echo HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n"
                          b"2;name=value\r\n" + BINARY[:2] + b"\r\n2\r\n" + BINARY[2:] + b"\r\n0\r\n\r\n")
        self.assertEqual(result[::2], (200, BINARY))
        result = self.raw(b"POST /trailers HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n"
                          b"0\r\nX-Trailer: yes\r\n\r\n")
        self.assertEqual(result[::2], (200, b"yes"))

    def test_continue_handshake(self):
        with socket.create_connection(("127.0.0.1", self.server.port), 3) as sock:
            sock.sendall(b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\nExpect: 100-continue\r\n\r\n")
            with sock.makefile("rb") as stream:
                self.assertEqual(raw_response(stream)[0], 100)
                sock.sendall(BINARY)
                self.assertEqual(raw_response(stream)[::2], (200, BINARY))

    def test_head_empty_statuses_and_http10(self):
        self.assertEqual(self.server.request("HEAD", "/health")[::2], (200, b""))
        self.assertEqual(self.server.request("GET", "/no-content")[::2], (204, b""))
        self.assertEqual(self.server.request("GET", "/not-modified")[::2], (304, b""))
        self.assertEqual(self.raw(b"GET /health HTTP/1.0\r\n\r\n")[::2], (200, b"ok"))

    def test_connection_request_limit(self):
        with socket.create_connection(("127.0.0.1", self.server.port), 3) as sock:
            sock.sendall(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n" * 5)
            with sock.makefile("rb") as stream:
                for i in range(4):
                    status, headers, body = raw_response(stream)
                    self.assertEqual((status, body), (200, b"ok"))
                    self.assertEqual(headers[b"connection"], [b"close" if i == 3 else b"keep-alive"])
                self.assertEqual(stream.read(1), b"")

    def test_ambiguous_or_malformed_requests_close(self):
        requests = [
            b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n",
            b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\n",
            b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: -1\r\n\r\n",
            b"GET / HTTP/1.1\r\n\r\n", b"GET / HTTP/1.1\r\nHost: x\r\nHost: y\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: x\r\n folded: no\r\n\r\n", b"GET / HTTP/1.1\nHost: x\n\n",
            b"GET / HTTP/1.1\r\nHost: x\r\n\x80: no\r\n\r\n",
            b"GET / HTTP/1.0\r\nTransfer-Encoding: chunked\r\n\r\n",
            b"POST /echo HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nHost: evil\r\n\r\n",
        ]
        for request in requests:
            with self.subTest(request=request):
                status, headers, body = self.raw(request)
                self.assertEqual(status, 400)
                self.assertEqual(headers[b"connection"], [b"close"])

    def test_limits_expectations_and_truncation(self):
        for request, expected in [
            (b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 1025\r\n\r\n", 413),
            (b"POST /echo HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n401\r\n", 413),
            (b"GET / HTTP/1.1\r\nHost: x\r\nX-Long: " + b"a" * 300 + b"\r\n\r\n", 431),
            (b"GET / HTTP/1.1\r\nHost: x\r\n" + (b"X: " + b"a" * 180 + b"\r\n") * 15 + b"\r\n", 431),
            (b"POST /echo HTTP/1.1\r\nHost: x\r\nExpect: unsupported\r\n\r\n", 417),
        ]:
            with self.subTest(expected=expected):
                self.assertEqual(self.raw(request)[0], expected)
        self.assertEqual(self.raw(b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\na", True)[0], 400)
        self.assertEqual(self.raw(b"GET ")[0], 408)

    def test_invalid_handler_response_becomes_500(self):
        self.assertEqual(self.server.request("GET", "/bad-response")[0], 500)

    def test_concurrent_workers(self):
        start = time.monotonic()
        with concurrent.futures.ThreadPoolExecutor(4) as pool:
            results = list(pool.map(lambda _: self.server.request("GET", "/slow"), range(4)))
        self.assertTrue(all(r[0] == 200 and r[2] == b"slow" for r in results))
        self.assertLess(time.monotonic() - start, 0.55, "four handlers ran serially")

    def test_descriptors_reclaimed_and_disconnect_survives(self):
        def fd_count():
            return len(list(Path(f"/proc/{self.server.process.pid}/fd").iterdir()))
        time.sleep(0.15)
        before = fd_count()
        for _ in range(40):
            self.assertEqual(self.server.request("POST", "/echo", BINARY)[2], BINARY)
        with socket.create_connection(("127.0.0.1", self.server.port), 3) as sock:
            sock.sendall(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n")
        time.sleep(0.2)
        self.assertLessEqual(fd_count(), before)
        self.assertEqual(self.server.request("GET", "/health")[0], 200)

    def test_stop_with_idle_connections(self):
        server = FibberServer()
        sock = socket.create_connection(("127.0.0.1", server.port), 3)
        try:
            sock.sendall(b"GET ")
            start = time.monotonic()
            server.close()
            self.assertLess(time.monotonic() - start, 1.5)
            try:
                self.assertEqual(sock.recv(1), b"")
            except ConnectionResetError:
                pass  # closed with the request's first bytes still unread by the server: a reset is a close too
        finally:
            sock.close()

    def test_owned_objects_reclaimed_after_requests(self):
        server = FibberServer(trace=True)
        try:
            self.assertEqual(server.request("POST", "/echo", BINARY)[2], BINARY)
            self.assertEqual(server.request("GET", "/bad-response")[0], 500)
            self.assertEqual(server.request("GET", "/missing")[0], 404)
        finally:
            trace = server.close()
        audit_owned_objects(trace)

    def test_stop_with_a_peer_that_does_not_read(self):
        server = FibberServer()
        with socket.create_connection(("127.0.0.1", server.port), 3) as sock:
            try:
                sock.sendall(b"GET /large HTTP/1.1\r\nHost: x\r\n\r\n")
                self.assertTrue(sock.recv(128).startswith(b"HTTP/1.1 200"))
                start = time.monotonic()
            finally:
                server.close()
            self.assertLess(time.monotonic() - start, 1.5)

    def test_response_deadline_closes_a_stalled_writer(self):
        server = FibberServer()
        try:
            with socket.create_connection(("127.0.0.1", server.port), 3) as sock:
                sock.sendall(b"GET /large HTTP/1.1\r\nHost: x\r\n\r\n")
                received = len(sock.recv(128))
                time.sleep(0.8)
                while data := sock.recv(65536):
                    received += len(data)
                self.assertLess(received, 8388608, "stalled response exceeded its deadline")
        finally:
            server.close()


class PythonHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def handle(self):
        try:
            super().handle()
        except (ConnectionResetError, BrokenPipeError):
            # Expected when testing TLS rejection, limits, and timeouts.
            pass

    def log_message(self, *args):
        pass

    def dispatch(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        path = self.path.split("?", 1)[0]
        status, response = 200, BINARY
        if path == "/echo":
            response = body
        elif path == "/auth":
            response = self.headers.get("Authorization", "").encode()
        elif path == "/query":
            response = self.path.split("?", 1)[-1].encode()
        elif path == "/missing":
            status, response = 404, b"missing"
        elif path == "/redirect":
            status, response = 302, b"redirecting"
        elif path == "/slow":
            time.sleep(0.15)
        elif path == "/gzip":
            response = gzip.compress(b"decompressed \xce\xbb")
        self.send_response(status)
        self.send_header("X-Peer", str(self.client_address[1]))
        self.send_header("Set-Cookie", "a=1")
        self.send_header("Set-Cookie", "b=2")
        if path == "/redirect":
            self.send_header("Location", "/binary")
        if path == "/gzip":
            self.send_header("Content-Encoding", "gzip")
        if path == "/large-header":
            self.send_header("X-Large", "x" * 500)
        if path == "/chunked":
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            response = b"2\r\n" + BINARY[:2] + b"\r\n2\r\n" + BINARY[2:] + b"\r\n0\r\nX-Trailer: yes\r\n\r\n"
        else:
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
        if self.command != "HEAD":
            try:
                self.wfile.write(response)
            except (BrokenPipeError, ConnectionResetError):
                pass

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = dispatch


class ClientTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), PythonHandler)
        cls.server.daemon_threads = True
        cls.thread = threading.Thread(target=cls.server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
        cls.thread.start()
        cls.base = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def call(self, path, **options):
        return client(self.base + path, **options)

    def test_binary_response_repeated_headers_and_chunked(self):
        for path in ("/binary", "/chunked"):
            result = self.call(path)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(fields(result.stdout)["body"], [BINARY.hex()])
            self.assertEqual(fields(result.stdout)["cookies"], ["a=1b=2"])

    def test_binary_post_forms_and_verbs(self):
        result = self.call("/echo", mode="binary", method="POST")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(fields(result.stdout)["body"], [BINARY.hex()])
        result = self.call("/echo", mode="form")
        self.assertEqual(fields(result.stdout)["body"], [b"q=a+b%2B%C3%A9&tag=x&tag=y".hex()])
        for method in ("PUT", "PATCH", "DELETE"):
            result = self.call("/echo", method=method, body="hello")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(fields(result.stdout)["body"], [b"hello".hex()])
        result = self.call("/binary", method="HEAD")
        self.assertEqual(fields(result.stdout)["body"], [""])

    def test_status_errors_preserve_body(self):
        result = self.call("/missing")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(fields(result.stdout)["status"], ["404"])
        self.assertEqual(fields(result.stdout)["body"], [b"missing".hex()])
        self.assertEqual(self.call("/missing", throw=0).returncode, 0)

    def test_redirect_policy_and_post_302(self):
        result = self.call("/redirect")
        self.assertEqual(fields(result.stdout)["status"], ["302"])
        for options in ({"follow": 1}, {"follow": 1, "method": "POST", "body": "hello"}):
            result = self.call("/redirect", **options)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(fields(result.stdout)["status"], ["200"])
            self.assertEqual(fields(result.stdout)["body"], [BINARY.hex()])

    def test_response_size_boundary_and_header_limit(self):
        self.assertEqual(self.call("/binary", limit=4).returncode, 0)
        self.assertEqual(self.call("/binary", limit=3).returncode, 3)
        self.assertEqual(self.call("/binary", limit=0).returncode, 3)
        self.assertEqual(self.call("/large-header", header_limit=256).returncode, 3)

    def test_compression_and_bearer_auth(self):
        # No zlib driver is registered in the fixture client: the body arrives as received (the Decoder seam is covered by specs/http-client-spec.fib).
        result = self.call("/gzip")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(gzip.decompress(bytes.fromhex(fields(result.stdout)["body"][0])), b"decompressed \xce\xbb")
        result = self.call("/gzip", decompress=0)
        self.assertEqual(gzip.decompress(bytes.fromhex(fields(result.stdout)["body"][0])), b"decompressed \xce\xbb")
        result = self.call("/auth", mode="bearer")
        self.assertEqual(fields(result.stdout)["body"], [b"Bearer test-token".hex()])
        result = self.call("/auth", mode="basic")
        self.assertEqual(fields(result.stdout)["body"], [b"Basic dXNlcm5hbWU6cGFzc3dvcmQ=".hex()])

    def test_connection_reuse_and_closed_client(self):
        result = self.call("/binary", mode="reuse")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        peers = fields(result.stdout)["peer"]
        self.assertEqual(len(peers), 3)
        self.assertEqual(len(set(peers)), 1, "client opened a fresh connection for each request")
        self.assertEqual(self.call("/binary", mode="closed").returncode, 4)

    def test_async_concurrency_and_timeout(self):
        start = time.monotonic()
        result = self.call("/slow", mode="async")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(fields(result.stdout)["status"], ["200"] * 4)
        self.assertLess(time.monotonic() - start, 0.55, "async requests ran serially")
        self.assertEqual(self.call("/slow", timeout=20).returncode, 5)

    def test_fibber_client_to_fibber_server(self):
        server = FibberServer()
        try:
            result = client(f"http://127.0.0.1:{server.port}/echo", mode="binary", method="POST")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(fields(result.stdout)["body"], [BINARY.hex()])
        finally:
            server.close()

    def test_owned_objects_reclaimed_on_success_and_errors(self):
        for path, options, expected in [
            ("/binary", {}, 0), ("/missing", {}, 2), ("/binary", {"limit": 3}, 3),
            ("/binary", {"mode": "reuse"}, 0), ("/binary", {"mode": "async"}, 0),
        ]:
            with self.subTest(path=path, options=options):
                result = self.call(path, trace=True, **options)
                self.assertEqual(result.returncode, expected, result.stdout)
                audit_owned_objects(result.stderr)

    def test_https_is_refused_until_a_tls_driver_exists(self):
        with tempfile.TemporaryDirectory(prefix="fibber-http-tls-") as directory:
            cert, key = Path(directory) / "cert.pem", Path(directory) / "key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost",
                "-keyout", str(key), "-out", str(cert)], capture_output=True, check=True, timeout=10)
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(cert, key)
            server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), PythonHandler)
            server.socket = context.wrap_socket(server.socket, server_side=True)
            thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
            thread.start()
            try:
                url = f"https://localhost:{server.server_port}/binary"
                # TLS is the next package (TLS-1): until a driver is registered an https URL is a typed error, and nothing is sent in the clear.
                for result in (client(url), client(url, ca=cert), client(f"https://127.0.0.1:{server.server_port}/binary", ca=cert)):
                    self.assertEqual(result.returncode, 5, result.stdout)
                    self.assertIn("unsupported scheme https", result.stdout + result.stderr)
            finally:
                server.shutdown()
                server.server_close()
                thread.join()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--client", required=True)
    parser.add_argument("--server", required=True)
    ARGS, remaining = parser.parse_known_args()
    unittest.main(argv=[__file__] + remaining, verbosity=2)
