#!/usr/bin/env python3
"""A second TLS implementation to test fib.tls against: python's ssl module (OpenSSL under it, but another program: its own handshake driver and its own HTTP). usage:
tls-pyserver.py PORT CERT KEY [CA]   TLS 1.3 only. GET / -> a short page; GET /big -> 1 MiB of the pattern the client checks; POST /echo -> the number of bytes received;
GET /trunc -> a partial body, then the TCP connection is closed WITHOUT close_notify. CA, when given, asks the client for a certificate (and does not require it)."""
import socket, ssl, sys, threading

port, cert, key = int(sys.argv[1]), sys.argv[2], sys.argv[3]
ca = sys.argv[4] if len(sys.argv) > 4 else None
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.minimum_version = ssl.TLSVersion.TLSv1_3
ctx.load_cert_chain(cert, key)
ctx.set_alpn_protocols(["http/1.1"])
if ca:
    ctx.load_verify_locations(ca)
    ctx.verify_mode = ssl.CERT_OPTIONAL


def pattern(n):
    return bytes(97 + (i * 7) % 26 for i in range(n))


def handle(conn):
    clean = True
    try:
        data = b""
        while b"\r\n\r\n" not in data:
            chunk = conn.recv(65536)
            if not chunk:
                return
            data += chunk
        head, _, rest = data.partition(b"\r\n\r\n")
        line = head.split(b"\r\n")[0].decode()
        method, path = line.split(" ")[:2]
        if method == "POST":
            n = int([h.split(b":")[1] for h in head.split(b"\r\n") if h.lower().startswith(b"content-length")][0])
            while len(rest) < n:
                chunk = conn.recv(65536)
                if not chunk:
                    break
                rest += chunk
            body = ("received %d\n" % len(rest)).encode()
        elif path == "/trunc":
            conn.sendall(b"HTTP/1.0 200 OK\r\nContent-Length: 100000\r\n\r\npartial")
            clean = False   # the `finally` below shuts the TCP connection down WITHOUT close_notify: a truncation
            return
        elif path == "/big":
            body = pattern(1048576)
        else:
            body = b"hello from python ssl\n"
        conn.sendall(b"HTTP/1.0 200 OK\r\nContent-Length: %d\r\n\r\n" % len(body) + body)
    finally:
        try:
            if clean:
                conn = conn.unwrap()   # sends close_notify
            conn.shutdown(socket.SHUT_RDWR)
        except Exception:
            pass
        try:
            conn.close()
        except Exception:
            pass


srv = socket.socket()
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", port))
srv.listen(16)
while True:
    raw, _ = srv.accept()
    try:
        tls = ctx.wrap_socket(raw, server_side=True)
    except (ssl.SSLError, OSError) as e:
        sys.stderr.write("handshake failed: %s\n" % e)
        raw.close()
        continue
    threading.Thread(target=handle, args=(tls,), daemon=True).start()
