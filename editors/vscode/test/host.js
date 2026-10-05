'use strict';
// The real client: downloads (once, into the cache directory) a VS Code with @vscode/test-electron, installs nothing on the system, starts it
// headless under xvfb-run when there is no display, loads this extension from this directory (so vscode-languageclient is the one in
// node_modules) and runs test/host/index.js in its extension host against `fibc lsp`.
//
// usage: node test/host.js          FIBC names the fibc (default `fibc` on the PATH); FIB_LIB the library; VSCODE_CACHE where VS Code is kept
//        (default $HOME/.cache/fibber-vscode); `npm install` first (vscode-languageclient) and `npm install --no-save @vscode/test-electron`.
// Exit: 0 passed; 1 failed; 2 not possible here (no @vscode/test-electron, or no xvfb-run and no DISPLAY): said in words, not a pass.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

let runTests;
try { ({ runTests } = require('@vscode/test-electron')); } catch (_) {
  console.error('host test: @vscode/test-electron is not installed (npm install --no-save @vscode/test-electron)'); process.exit(2);
}
if (!process.env.DISPLAY && process.argv[2] !== '--inner') {
  const x = spawnSync('xvfb-run', ['-a', process.execPath, __filename, '--inner'], { stdio: 'inherit', env: process.env });
  if (x.error) { console.error('host test: no DISPLAY and no xvfb-run: cannot start VS Code'); process.exit(2); }
  process.exit(x.status === null ? 1 : x.status);
}

const ws = fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(), 'fibber-host-'));
fs.writeFileSync(path.join(ws, 'good.fib'), '(defstruct P (x: i64))\n(defun inc (k: i64) -> i64 (+ k 1))\n(def limit: i64 3)\n(defun g () -> i64 (inc 1))\n');
fs.writeFileSync(path.join(ws, 'bad.fib'), '(ns bad)\n(defun f () -> i64 "s")\n');
fs.mkdirSync(path.join(ws, '.vscode'));
const fibc = process.env.FIBC || 'fibc';
fs.writeFileSync(path.join(ws, '.vscode', 'settings.json'), JSON.stringify({ 'fibber.serverPath': fibc, 'fibber.libraryPath': process.env.FIB_LIB || '' }));

(async () => {
  try {
    await runTests({
      version: process.env.VSCODE_VERSION || 'stable',
      cachePath: process.env.VSCODE_CACHE || path.join(os.homedir(), '.cache', 'fibber-vscode'),
      extensionDevelopmentPath: path.resolve(__dirname, '..'),
      extensionTestsPath: path.resolve(__dirname, 'host', 'index.js'),
      extensionTestsEnv: { FIBBER_TEST_WORKSPACE: ws },
      launchArgs: [ws, '--disable-extensions', '--disable-gpu', '--no-sandbox', '--user-data-dir', path.join(ws, 'user-data')],
    });
    console.log('host test: ok');
    process.exit(0);
  } catch (e) {
    console.error('host test: FAILED', e && e.message ? e.message : e);
    process.exit(1);
  }
})();
