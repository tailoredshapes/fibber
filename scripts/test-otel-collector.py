#!/usr/bin/env python3
"""Real pinned collector acceptance. Caller holds /tmp/fibsuite.lock read-mode LOCK_EX.

Accepts a prebuilt F or Gloss fixture; never builds or substitutes a fake receiver.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time
import urllib.request
import urllib.error
import uuid

ROOT = Path(__file__).resolve().parent.parent
IMAGE = (ROOT/'scripts/otel-collector-image').read_text().strip()
if not re.fullmatch(r'otel/opentelemetry-collector-contrib@sha256:[0-9a-f]{64}', IMAGE):
    raise RuntimeError('collector image must be pinned by SHA256 digest')


def require(value, message):
    if not value:
        raise RuntimeError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def docker(*args):
    return subprocess.check_output(['docker', *args], text=True, stderr=subprocess.STDOUT, timeout=30).strip()


def stop_group(child):
    for number in [signal.SIGTERM, signal.SIGKILL]:
        try:
            os.killpg(child.pid, number)
        except ProcessLookupError:
            break
        try:
            child.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass
    child.wait(timeout=3)


def fixture(binary, base, destination):
    env = dict(os.environ, OTLP_TEST_BASE=base)
    for key in ['CPUPROFILE', 'CPUPROFILESIGNAL', 'LD_PRELOAD']:
        env.pop(key, None)
    with destination.open('wb') as log:
        child = subprocess.Popen([str(binary)], env=env, stdout=log,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = child.wait(timeout=30)
        except BaseException:
            stop_group(child)
            raise
    return code


def interrupted(number, frame):
    raise KeyboardInterrupt('collector acceptance interrupted')


def wait_ready(name, port):
    deadline = time.monotonic()+20
    error = 'no HTTP response'
    while time.monotonic() < deadline:
        require(docker('inspect', '--format', '{{.State.Running}}', name) == 'true', 'collector exited')
        try:
            request = urllib.request.Request('http://127.0.0.1:'+str(port)+'/v1/traces',
                                             method='GET')
            with urllib.request.urlopen(request, timeout=0.5) as response:
                error = 'unexpected readiness status '+str(response.status)
        except urllib.error.HTTPError as failure:
            if failure.code == 405:
                return
            error = 'readiness HTTP '+str(failure.code)
        except (OSError, ValueError) as failure:
            error = str(failure)
            time.sleep(0.1)
    raise RuntimeError('collector HTTP readiness timeout: '+error)


def records(output, resource_key, scope_key, item_key):
    result = []
    for line in output.read_text().splitlines():
        for resource in json.loads(line).get(resource_key, []):
            attrs = {a['key']: a['value'] for a in resource['resource']['attributes']}
            require(attrs['service.name'] == {'stringValue': 'fibber-collector-acceptance'}, 'resource lost')
            for scope in resource[scope_key]:
                for item in scope[item_key]:
                    result.append((scope['scope']['name'], item))
    return result


def attrs(item):
    return {a['key']: a['value'] for a in item.get('attributes', [])}


def typed(attributes):
    require(attributes['huge'] == {'intValue': '9223372036854775807'}, 'i64 precision/type lost')
    require(attributes['bytes'] == {'bytesValue': base64.b64encode(b'hi').decode()}, 'bytes encoding lost')


def traces(output):
    spans = records(output/'traces.json', 'resourceSpans', 'scopeSpans', 'spans')
    require(len(spans) == 2, 'trace count mismatch')
    by_name = {item['name']: item for scope, item in spans}
    require(set(by_name) == {'root', 'child'}, 'trace names mismatch')
    for scope, item in spans:
        require(scope == 'fixture.trace', 'trace scope lost')
        require(item['traceId'] == '11111111111111111111111111111111', 'trace ID corrupted')
        require(item['status'] == {'message': 'fixture failure', 'code': 2}, 'status lost')
        require(item['traceState'] == 'vendor=value', 'tracestate lost')
        require(item['startTimeUnixNano'] == '1700000000000000000', 'timestamp precision lost')
        require(item['events'][0]['name'] == 'ready', 'event lost')
        typed(attrs(item))
    require(by_name['root']['spanId'] == '0000000000000011', 'root ID corrupted')
    require(by_name['child']['spanId'] == '0000000000000022', 'child ID corrupted')
    require(by_name['child']['parentSpanId'] == by_name['root']['spanId'], 'parentage lost')
    require(by_name['child']['flags'] & 0x301 == 0x301, 'remote/sampling flags lost')


def metrics(output):
    data = records(output/'metrics.json', 'resourceMetrics', 'scopeMetrics', 'metrics')
    require(len(data) == 2 and all(s == 'fixture.metrics' for s, _ in data), 'metric count/scope lost')
    by_name = {m['name']: m for _, m in data}
    total = by_name['requests']['sum']
    require(total['aggregationTemporality'] == 2 and total['isMonotonic'], 'counter semantics lost')
    require(total['dataPoints'][0]['asInt'] == '9223372036854775807', 'counter i64 precision lost')
    hist = by_name['latency']['histogram']['dataPoints'][0]
    require(hist['count'] == '4' and hist['bucketCounts'] == ['1', '2', '1'], 'histogram counts lost')
    require(hist['explicitBounds'] == [1, 2] and hist['sum'] == 7, 'histogram bounds/sum lost')


def logs(output):
    data = records(output/'logs.json', 'resourceLogs', 'scopeLogs', 'logRecords')
    require(len(data) == 1, 'log count mismatch')
    scope, item = data[0]
    require(scope == 'fixture.log' and item['body'] == {'stringValue': 'collector accepted'}, 'log body/scope lost')
    require(item['traceId'] == '11111111111111111111111111111111' and item['spanId'] == '0000000000000022', 'log correlation lost')
    require(item['severityNumber'] == 9 and item['severityText'] == 'INFO', 'log severity lost')
    typed(attrs(item))


def validate(output):
    deadline = time.monotonic()+10
    while time.monotonic() < deadline:
        if all((output/(name+'.json')).exists() and (output/(name+'.json')).stat().st_size for name in ['traces', 'metrics', 'logs']):
            traces(output)
            metrics(output)
            logs(output)
            return
        time.sleep(0.1)
    raise RuntimeError('collector file exports missing')


def exercise(args, name, output, report):
    config = ROOT/'specs/fixtures/otel-collector/config.yaml'
    image = json.loads(docker('image', 'inspect', IMAGE))[0]
    report.update(image=IMAGE, image_id=image['Id'], config_sha256=sha(config))
    docker('run', '-d', '--pull=never', '--name', name, '-p', '127.0.0.1::4318',
           '-v', str(config)+':/etc/otelcol-contrib/config.yaml:ro', '-v', str(output)+':/output:rw', IMAGE)
    port = int(docker('port', name, '4318/tcp').rsplit(':', 1)[1])
    wait_ready(name, port)
    base = 'http://127.0.0.1:'+str(port)
    require(fixture(args.binary, base, args.output/'fixture.log') == 0, 'fixture delivery failed')
    validate(output)
    sizes = {p.name: p.stat().st_size for p in output.glob('*.json')}
    require(fixture(args.binary, base+'/wrong-route', args.output/'negative.log') == 1, 'wrong-route control did not fail')
    time.sleep(0.3)
    require(sizes == {p.name: p.stat().st_size for p in output.glob('*.json')}, 'wrong route unexpectedly exported telemetry')
    report.update(passed=True, traces=2, metrics=2, logs=1, wrong_route_rejected=True)


def cleanup(name, destination):
    result = subprocess.run(['docker', 'logs', name], capture_output=True, text=True, timeout=15)
    (destination/'collector.log').write_text(result.stdout+result.stderr)
    removed = subprocess.run(['docker', 'rm', '-f', name], capture_output=True, text=True, timeout=15)
    remaining = subprocess.run(['docker', 'ps', '-aq', '--filter', 'name=^/'+name+'$'],
                               capture_output=True, text=True, timeout=15)
    require(remaining.returncode == 0 and not remaining.stdout.strip(), 'owned collector survived cleanup')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.binary, args.output = args.binary.resolve(), args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    output = args.output/'collector-output'
    output.mkdir(mode=0o777)
    os.chmod(output, 0o777)
    name = 'fibber-otel-acceptance-'+uuid.uuid4().hex[:12]
    report = dict(passed=False, binary=str(args.binary), binary_sha256=sha(args.binary), container=name)
    signal.signal(signal.SIGTERM, interrupted)
    try:
        exercise(args, name, output, report)
        require(sha(args.binary) == report['binary_sha256'], 'fixture binary changed')
    except Exception as error:
        report.update(passed=False, error=str(error))
    finally:
        try:
            cleanup(name, args.output)
            report['owned_container_gone'] = True
        except Exception as error:
            report.update(passed=False, owned_container_gone=False, cleanup_error=str(error))
        finally:
            (args.output/'results.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
