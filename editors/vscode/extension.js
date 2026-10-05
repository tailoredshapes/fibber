'use strict';
// The fibber extension: starts the language server (`fibc lsp` by default, or `fibref lsp`) and lets vscode-languageclient
// speak to it (completion, hover, diagnostics, go to definition, document symbols). Plain JS, no bundler.

const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

let client;
let warning;

/** The command line and environment the settings ask for. */
function serverOptions() {
  const cfg = vscode.workspace.getConfiguration('fibber');
  const args = ['lsp'];
  for (const dir of cfg.get('includePaths', [])) args.push('-I', dir);
  const env = { ...process.env };
  const lib = cfg.get('libraryPath', '');
  if (lib) env.FIB_LIB = lib;
  // `fibber.fibrefPath` (the old setting, empty by default) wins when set, so a configuration that named a fibref keeps working.
  const command = cfg.get('fibrefPath', '') || cfg.get('serverPath', 'fibc') || 'fibc';
  return { command, args, options: { env } };
}

/** One quiet status-bar item when the server cannot start. */
function warn(reason) {
  if (!warning) {
    warning = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
    warning.command = 'fibber.restartServer';
  }
  warning.text = '$(warning) fibber: no language server';
  warning.tooltip = `fibber: ${reason}. Set fibber.serverPath, then click to restart.`;
  warning.show();
}

async function start() {
  if (warning) warning.hide();
  client = new LanguageClient('fibber', 'fibber', serverOptions(), {
    documentSelector: [
      { scheme: 'file', language: 'fibber' },
      { scheme: 'untitled', language: 'fibber' },
    ],
    revealOutputChannelOn: 4, // RevealOutputChannelOn.Never: no output-channel spam
  });
  try {
    await client.start();
  } catch (e) {
    client = undefined;
    warn(e && e.message ? e.message : String(e));
  }
}

async function stop() {
  const c = client;
  client = undefined;
  if (c) {
    try {
      await c.stop();
    } catch (_) {
      // a server that never started has nothing to stop
    }
  }
}

async function restart() {
  await stop();
  await start();
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand('fibber.restartServer', restart),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('fibber')) restart();
    }),
    { dispose: () => warning && warning.dispose() }
  );
  return start();
}

function deactivate() {
  return stop();
}

module.exports = { activate, deactivate };
