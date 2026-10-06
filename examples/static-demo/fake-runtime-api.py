#!/usr/bin/env python3
"""A tiny local fake of the AWS Lambda Runtime API (https://docs.aws.amazon.com/lambda/latest/dg/runtimes-api.html), for the bootstrap demo.

usage: fake-runtime-api.py PORT EVENTS.json   serves each event of the JSON list once on GET /2018-06-01/runtime/invocation/next (with the
Lambda-Runtime-Aws-Request-Id header), prints what a runtime POSTs to .../invocation/ID/response, and exits after the last response.
Exit status 0 when every event got a response, 1 otherwise. No AWS account, no network beyond the local port."""
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

events = json.load(open(sys.argv[2]))
state = {"next": 0, "answered": {}}

class Api(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"          # the connection closes after each response, as the bootstrap expects

    def do_GET(self):
        if self.path != "/2018-06-01/runtime/invocation/next" or state["next"] >= len(events):
            self.send_response(404); self.end_headers(); return
        i = state["next"]; state["next"] += 1
        body = json.dumps(events[i]).encode()
        self.send_response(200)
        self.send_header("Lambda-Runtime-Aws-Request-Id", "req-%d" % i)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers(); self.wfile.write(body)

    def do_POST(self):
        n = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(n).decode()
        rid = self.path.split("/")[-2]
        print("fake runtime API: response for %s: %s" % (rid, body), flush=True)
        state["answered"][rid] = body
        self.send_response(202); self.send_header("Content-Length", "0"); self.end_headers()
        if len(state["answered"]) == len(events):
            self.server.done = True

    def log_message(self, *a): pass

srv = HTTPServer(("0.0.0.0", int(sys.argv[1])), Api)
srv.done = False
while not srv.done:
    srv.handle_request()
srv.server_close()
sys.exit(0 if len(state["answered"]) == len(events) else 1)
