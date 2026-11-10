// upstream: src/vs/workbench/api/common/extHostExtensionService.ts @ 50e1350fc78d
// A small stand-in for ErrorLens, written for this repo: it touches the extension APIs ErrorLens
// uses (commands, configuration, a status bar item, text editor decorations, diagnostics) and
// reports what it saw through the `fixture.report` command, so tests can read the results back
// over the wire. Plain CommonJS, no dependencies.
const vscode = require('vscode');

const seen = { activations: 0, configChanges: [], scans: 0 };

function activate(context) {
  if (process.env.FIXTURE_FAIL) {
    throw new Error('fixture failed on purpose');
  }
  seen.activations += 1;

  const cfg = () => vscode.workspace.getConfiguration('fixture');
  seen.enabled = cfg().get('enabled');
  seen.message = cfg().get('message');
  seen.statusPrefix = cfg().get('statusPrefix');

  const item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  item.text = `${seen.statusPrefix} 0`;
  item.tooltip = 'Fixture status';
  item.command = 'fixture.hello';
  item.show();

  const decoration = vscode.window.createTextEditorDecorationType({
    isWholeLine: true,
    backgroundColor: 'rgba(255, 0, 0, 0.1)',
    after: { margin: '0 0 0 2em', color: '#ff0000' },
  });
  const diagnostics = vscode.languages.createDiagnosticCollection('fixture');

  // Like ErrorLens: every line with ERROR becomes a diagnostic and a decoration with the
  // message after the line; the status bar item shows the count.
  function scan(editor) {
    if (!editor) {
      return 0;
    }
    seen.scans += 1;
    const doc = editor.document;
    const diags = [];
    const decorations = [];
    for (let i = 0; i < doc.lineCount; i++) {
      const line = doc.lineAt(i);
      if (line.text.includes('ERROR')) {
        diags.push(new vscode.Diagnostic(line.range, line.text.trim(), vscode.DiagnosticSeverity.Error));
        decorations.push({
          range: line.range,
          renderOptions: { after: { contentText: ` ${line.text.trim()}` } },
        });
      }
    }
    diagnostics.set(doc.uri, diags);
    editor.setDecorations(decoration, decorations);
    item.text = `${cfg().get('statusPrefix')} ${diags.length}`;
    return diags.length;
  }

  context.subscriptions.push(
    item,
    decoration,
    diagnostics,
    vscode.commands.registerCommand('fixture.hello', (who) => `hello ${who}`),
    vscode.commands.registerCommand('fixture.report', async () => {
      seen.viaExecute = await vscode.commands.executeCommand('fixture.hello', 'world');
      seen.message = cfg().get('message');
      return seen;
    }),
    vscode.commands.registerCommand('fixture.scan', () => scan(vscode.window.activeTextEditor)),
    // An app command, run by the main thread.
    vscode.commands.registerCommand('fixture.callApp', () =>
      vscode.commands.executeCommand('rscode.test.appCommand', 1, 'two')),
    vscode.commands.registerCommand('fixture.updateConfig', () => cfg().update('message', 'from the extension', true)),
    vscode.commands.registerCommand('fixture.saveActive', () => vscode.window.activeTextEditor.document.save()),
    vscode.window.onDidChangeActiveTextEditor((e) => scan(e)),
    vscode.workspace.onDidChangeTextDocument(() => scan(vscode.window.activeTextEditor)),
    vscode.workspace.onDidChangeConfiguration((e) => {
      seen.configChanges.push(e.affectsConfiguration('fixture.message'));
      item.text = `${cfg().get('statusPrefix')} ${item.text.split(' ').pop()}`;
    }),
  );
  scan(vscode.window.activeTextEditor);
  seen.apisCreated = true;
}

function deactivate() {}

module.exports = { activate, deactivate };
