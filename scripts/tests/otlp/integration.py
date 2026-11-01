#!/usr/bin/env python3
"""Real TCP fault receiver. Its independent JSON assertions are the wire oracle, not another Fibber renderer."""
import argparse
import collections
import json
import os
import signal
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Receiver(BaseHTTPRequestHandler):
    counts = collections.Counter()
    records = collections.defaultdict(list)

    def log_message(self, *args):
        pass

    def do_POST(self):
        raw = self.rfile.read(int(self.headers["Content-Length"]))
        payload = json.loads(raw)
        self.counts[self.path] += 1
        self.records[self.path].append((dict(self.headers), payload, time.monotonic()))
        n = self.counts[self.path]
        code, body, extra = 200, {}, {}
        if self.path == "/disconnect":
            self.close_connection = True
            return
        if self.path == "/retry" and n == 1:
            code, extra = 503, {"Retry-After": "0"}
        if self.path == "/seconds" and n == 1:
            code, extra = 429, {"Retry-After": "1"}
        if self.path == "/deadline":
            code, extra = 503, {"Retry-After": "120"}
        if self.path == "/terminal":
            code = 400
        if self.path == "/partial":
            body = {"partialSuccess": {"rejectedSpans": "1", "errorMessage": "do-not-print-collector-secrets"}}
        if self.path == "/timeout":
            time.sleep(0.4)
        encoded = b"invalid-json" if self.path == "/malformed" else json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        for key, value in extra.items():
            self.send_header(key, value)
        self.end_headers()
        try:
            self.wfile.write(encoded)
        except (BrokenPipeError, ConnectionResetError):
            pass


def run(client, base, mode):
    env = dict(os.environ, OTLP_TEST_BASE=base, OTLP_TEST_MODE=mode)
    process = subprocess.Popen([client], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=15)
    except BaseException:
        os.killpg(process.pid, signal.SIGKILL)
        process.communicate()
        raise
    assert process.returncode == 0, (mode, process.returncode, stderr)
    assert "do-not-print-collector-secrets" not in stdout + stderr
    assert "fixture-secret" not in stdout + stderr
    return json.loads(stdout.strip())


def attributes(items):
    return {item["key"]: item["value"] for item in items}


def verify_delivery():
    for path in ("/v1/traces", "/v1/metrics", "/v1/logs"):
        assert Receiver.counts[path] == 1, path
        headers, _, _ = Receiver.records[path][0]
        assert headers.get("content-type") == "application/json", headers
        assert headers.get("authorization") == "fixture-secret", headers
        assert "traceparent" not in headers and "tracestate" not in headers
    trace = Receiver.records["/v1/traces"][0][1]["resourceSpans"]
    assert len(trace) == 1 and len(trace[0]["scopeSpans"]) == 2
    spans = {s["name"]: s for scope in trace[0]["scopeSpans"] for s in scope["spans"]}
    root, child = spans["root"], spans["child"]
    assert "parentSpanId" not in root
    assert child["parentSpanId"] == "0000000000000007"
    assert child["traceId"] == "1234567890abcdef0000000000000001"
    assert child["spanId"] == "0000000000000065" and child["kind"] == 2
    assert child["flags"] == 769 and child["status"] == {"code": 2, "message": 'bad "quote"'}
    assert child["startTimeUnixNano"] == "1000000001" and child["endTimeUnixNano"] == "1000002501"
    assert child["events"][0]["timeUnixNano"] == "1000000101"
    assert child["links"][0]["flags"] == 769
    attrs = attributes(child["attributes"])
    assert attrs == {"huge": {"intValue": "9223372036854775807"}, "bytes": {"bytesValue": "aGk="}}
    metric = Receiver.records["/v1/metrics"][0][1]["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
    metric = {m["name"]: m for m in metric}
    assert metric["requests"]["sum"]["dataPoints"][0]["asInt"] == "9223372036854775807"
    assert metric["requests"]["sum"]["aggregationTemporality"] == 2
    assert metric["active"]["sum"]["isMonotonic"] is False
    point = metric["latency"]["histogram"]["dataPoints"][0]
    assert point["count"] == "4" and point["bucketCounts"] == ["1", "2", "1"]
    assert point["explicitBounds"] == [1, 2] and point["sum"] == 7
    assert metric["temperature"]["gauge"]["dataPoints"][0]["asDouble"] == 12.5
    log = Receiver.records["/v1/logs"][0][1]["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]
    assert log["timeUnixNano"] == "2000" and log["severityNumber"] == 9
    assert log["body"] == {"stringValue": 'hello "collector"'}
    assert log["traceId"] == child["traceId"] and log["spanId"] == child["spanId"]
    assert attributes(log["attributes"]) == {"answer": {"intValue": "42"}}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--client", required=True)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", 0), Receiver)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = f"http://127.0.0.1:{server.server_port}"
    try:
        result = run(args.client, base, "delivery")
        assert result["ok"] and result["logFailed"] == 0, result
        verify_delivery()
        print("otlp-tcp: delivery PASS")
        for mode, success, requests in [("retry", True, 2), ("seconds", True, 2),
                ("deadline", False, 1), ("terminal", False, 1), ("partial", True, 1),
                ("malformed", False, 1), ("disconnect", False, 3), ("timeout", False, 1)]:
            result = run(args.client, base, mode)
            assert result["ok"] is success and result["requests"] == requests, (mode, result)
            assert Receiver.counts["/" + mode] == requests, (mode, Receiver.counts)
            if mode == "partial":
                assert result["partial"] == 1 and result["rejected"] == 1 and result["retries"] == 0
            print(f"otlp-tcp: {mode} PASS")
        observed = Receiver.records["/seconds"]
        assert observed[1][2] - observed[0][2] >= 1.0, "Retry-After was not respected"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    main()
