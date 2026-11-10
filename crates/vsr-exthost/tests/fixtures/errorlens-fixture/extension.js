// upstream: src/vs/workbench/api/common/extHostExtensionService.ts @ 50e1350fc78d
// A small stand-in for ErrorLens, written for this repo: it touches the extension APIs ErrorLens
// uses (commands, configuration, a status bar item, text editor decorations, diagnostics) and
// reports what it saw through the `fixture.report` command, so tests can read the results back
// over the wire. Plain CommonJS, no dependencies.
const vscode = require('vscode');

const seen = { activations: 0 };

function activate(context) {
  if (process.env.FIXTURE_FAIL) {
    throw new Error('fixture failed on purpose');
  }
  seen.activations += 1;

  const cfg = vscode.workspace.getConfiguration('fixture');
  seen.enabled = cfg.get('enabled');
  seen.message = cfg.get('message');
  seen.statusPrefix = cfg.get('statusPrefix');

  context.subscriptions.push(
    vscode.commands.registerCommand('fixture.hello', (who) => `hello ${who}`),
    vscode.commands.registerCommand('fixture.report', async () => {
      seen.viaExecute = await vscode.commands.executeCommand('fixture.hello', 'world');
      return seen;
    }),
  );

  const item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  item.text = `${seen.statusPrefix} 0`;
  item.tooltip = 'Fixture status';
  item.command = 'fixture.hello';
  item.show();
  context.subscriptions.push(item);

  const decoration = vscode.window.createTextEditorDecorationType({
    isWholeLine: true,
    backgroundColor: 'rgba(255, 0, 0, 0.1)',
    after: { margin: '0 0 0 2em', color: '#ff0000' },
  });
  context.subscriptions.push(decoration);
  const diagnostics = vscode.languages.createDiagnosticCollection('fixture');
  context.subscriptions.push(diagnostics);
  seen.apisCreated = true;
}

function deactivate() {}

module.exports = { activate, deactivate };
