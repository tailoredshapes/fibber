'use strict';
// Runs INSIDE a real VS Code extension host (started by test/host.js through @vscode/test-electron): the fibber extension is activated by opening a
// .fib file, starts `fibc lsp` through vscode-languageclient, and the vscode API commands that ask the language client for completion, hover,
// definition, document symbols and diagnostics are run against it. Throws on the first failed assertion (the runner then exits 1).
const assert = require('node:assert');
const path = require('node:path');
const vscode = require('vscode');

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(what, f, ms = 60000) {
  const end = Date.now() + ms;
  for (;;) {
    const v = await f();
    if (v) return v;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await sleep(200);
  }
}

exports.run = async function run() {
  const dir = process.env.FIBBER_TEST_WORKSPACE;
  const good = vscode.Uri.file(path.join(dir, 'good.fib'));
  const bad = vscode.Uri.file(path.join(dir, 'bad.fib'));

  const doc = await vscode.workspace.openTextDocument(good);
  await vscode.window.showTextDocument(doc);
  assert.strictEqual(doc.languageId, 'fibber', 'the .fib file is the fibber language');
  // the extension activates, the client starts, the server answers: a request is the proof (the first can wait for the server's start)
  const symbols = await until('document symbols from the server', async () => {
    const s = await vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', good);
    return s && s.length ? s : null;
  });
  assert.deepStrictEqual(symbols.map((s) => s.name), ['P', 'inc', 'limit', 'g'], 'document symbols');

  // hover on `inc` at its use: the checker's scheme
  const line = doc.getText().split('\n').findIndex((l) => l.includes('(inc 1)'));
  const col = doc.getText().split('\n')[line].indexOf('inc');
  const hovers = await vscode.commands.executeCommand('vscode.executeHoverProvider', good, new vscode.Position(line, col + 1));
  const text = hovers.flatMap((h) => h.contents.map((c) => (typeof c === 'string' ? c : c.value))).join('\n');
  assert(/inc : /.test(text), `hover shows inc's type: ${text}`);

  // definition of `inc` at its use: the defun in this file
  const defs = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', good, new vscode.Position(line, col + 1));
  assert.strictEqual(defs.length, 1, 'one definition');
  const loc = defs[0];
  assert.strictEqual((loc.targetUri || loc.uri).fsPath, good.fsPath, 'in the same file');
  assert.strictEqual(((loc.targetRange || loc.range).start).line, 1, 'on the defun line');

  // completion after `(in`: the buffer's own `inc` and the library
  const comp = await vscode.commands.executeCommand('vscode.executeCompletionItemProvider', good, new vscode.Position(line, col + 1));
  const labels = comp.items.map((i) => (typeof i.label === 'string' ? i.label : i.label.label));
  assert(labels.includes('inc'), 'completion has inc');
  assert(labels.includes('map'), 'completion has the library');

  // diagnostics: none for the good file, the type error's range for the bad one
  assert.deepStrictEqual(vscode.languages.getDiagnostics(good), [], 'no diagnostics for the good file');
  const badDoc = await vscode.workspace.openTextDocument(bad);
  await vscode.window.showTextDocument(badDoc);
  const diags = await until('diagnostics of bad.fib', () => { const d = vscode.languages.getDiagnostics(bad); return d.length ? d : null; });
  assert.strictEqual(diags[0].range.start.line, 1, 'the error is on line 2');
  assert(diags[0].message.length > 0, 'a message');

  // an edit publishes new diagnostics (full-text sync): fix the file
  const edit = new vscode.WorkspaceEdit();
  edit.replace(bad, new vscode.Range(1, 0, 1, badDoc.lineAt(1).text.length), '(defun f () -> i64 1)');
  await vscode.workspace.applyEdit(edit);
  await until('the diagnostics to clear after the edit', () => vscode.languages.getDiagnostics(bad).length === 0);
};
