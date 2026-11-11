'use strict';
// A language client for the fake server, the way a stdio LSP client works: this extension spawns the
// server itself with `child_process` (no main-side process work), frames messages with Content-Length,
// sends `initialize`/`initialized` and `textDocument/didOpen`, turns `publishDiagnostics` into a
// diagnostic collection and forwards completion requests to the server.
const vscode = require('vscode');
const path = require('path');
const { spawn } = require('child_process');

const LANG = 'lsplang';
const SERVER = path.join(__dirname, 'fake-server.js');

class Client {
	constructor(serverPath) {
		this.child = spawn(process.execPath, [serverPath], { stdio: ['pipe', 'pipe', 'pipe'] });
		this.nextId = 1;
		this.pending = new Map();
		this.buffer = Buffer.alloc(0);
		this.onDiagnostics = () => undefined;
		this.stopped = false;
		this.child.stdout.on('data', chunk => this.onData(chunk));
		this.child.stderr.on('data', () => undefined);
		this.child.on('exit', () => {
			for (const p of this.pending.values()) { p.reject(new Error('language server exited')); }
			this.pending.clear();
		});
		this.child.stdin.on('error', () => undefined);
	}

	send(msg) {
		const body = Buffer.from(JSON.stringify(msg), 'utf8');
		this.child.stdin.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, 'ascii'), body]));
	}

	request(method, params) {
		const id = this.nextId++;
		return new Promise((resolve, reject) => {
			this.pending.set(id, { resolve, reject });
			this.send({ jsonrpc: '2.0', id, method, params });
		});
	}

	notify(method, params) {
		this.send({ jsonrpc: '2.0', method, params });
	}

	onData(chunk) {
		this.buffer = Buffer.concat([this.buffer, chunk]);
		for (;;) {
			const sep = this.buffer.indexOf('\r\n\r\n');
			if (sep < 0) { return; }
			const m = /Content-Length:\s*(\d+)/i.exec(this.buffer.subarray(0, sep).toString('ascii'));
			if (!m) { throw new Error('language server sent a frame without Content-Length'); }
			const start = sep + 4;
			const end = start + Number(m[1]);
			if (this.buffer.length < end) { return; }
			const msg = JSON.parse(this.buffer.subarray(start, end).toString('utf8'));
			this.buffer = this.buffer.subarray(end);
			this.dispatch(msg);
		}
	}

	dispatch(msg) {
		if (msg.method === undefined && msg.id !== undefined) {
			const p = this.pending.get(msg.id);
			if (!p) { return; }
			this.pending.delete(msg.id);
			if (msg.error) { p.reject(new Error(msg.error.message)); } else { p.resolve(msg.result); }
		} else if (msg.method === 'textDocument/publishDiagnostics') {
			this.onDiagnostics(msg.params);
		}
	}

	// `exit` ends the server; closing its stdin is the fallback (the server exits on EOF too).
	stop() {
		if (this.stopped) { return; }
		this.stopped = true;
		try { this.notify('exit', undefined); } catch (_) { /* already gone */ }
		this.child.stdin.end();
	}
}

function toRange(r) {
	return new vscode.Range(r.start.line, r.start.character, r.end.line, r.end.character);
}

let client;

exports.activate = function (ctx) {
	client = new Client(SERVER);
	const ready = client
		.request('initialize', { processId: process.pid, rootUri: null, capabilities: {} })
		.then(result => {
			client.notify('initialized', {});
			return result;
		});
	ready.catch(() => undefined);

	const diagnostics = vscode.languages.createDiagnosticCollection('lsp-fixture');
	ctx.subscriptions.push(diagnostics);
	client.onDiagnostics = params => {
		const uri = vscode.Uri.parse(params.uri);
		diagnostics.set(uri, params.diagnostics.map(d => {
			const diag = new vscode.Diagnostic(toRange(d.range), d.message, d.severity - 1);
			diag.source = d.source;
			return diag;
		}));
	};

	const opened = new Set();
	const open = doc => {
		const uri = doc.uri.toString();
		if (doc.languageId !== LANG || opened.has(uri)) { return; }
		opened.add(uri);
		ready.then(() => client.notify('textDocument/didOpen', {
			textDocument: { uri, languageId: doc.languageId, version: doc.version, text: doc.getText() },
		})).catch(() => undefined);
	};
	ctx.subscriptions.push(vscode.workspace.onDidOpenTextDocument(open));
	vscode.workspace.textDocuments.forEach(open);

	ctx.subscriptions.push(vscode.languages.registerCompletionItemProvider(LANG, {
		async provideCompletionItems(doc, pos) {
			await ready;
			const result = await client.request('textDocument/completion', {
				textDocument: { uri: doc.uri.toString() },
				position: { line: pos.line, character: pos.character },
				context: { triggerKind: 1 },
			});
			const list = result || { items: [], isIncomplete: false };
			const items = list.items.map(i => {
				const item = new vscode.CompletionItem(i.label, i.kind - 1);
				if (i.insertText) { item.insertText = i.insertText; }
				if (i.detail) { item.detail = i.detail; }
				return item;
			});
			return new vscode.CompletionList(items, !!list.isIncomplete);
		}
	}, '.'));

	// Reports what the test needs to check: where Node comes from, the server's pid, and this host's pid.
	ctx.subscriptions.push(vscode.commands.registerCommand('lsp-fixture.info', async () => {
		const init = await ready.catch(() => undefined);
		return {
			execPath: process.execPath,
			electron: process.versions.electron || null,
			hostPid: process.pid,
			serverPid: client.child.pid,
			serverName: init && init.serverInfo ? init.serverInfo.name : null,
		};
	}));

	// Spawns a tool by its bare name, the way a language server started from `command` does; the
	// answer is its stdout, or `missing` when the host's PATH does not have it (ADR-0100).
	ctx.subscriptions.push(vscode.commands.registerCommand('lsp-fixture.probe', () => {
		const r = require('child_process').spawnSync('rscode-probe', [], { encoding: 'utf8' });
		return r.error ? 'missing' : r.stdout.trim();
	}));

	ctx.subscriptions.push({ dispose: () => client.stop() });
};

// Runs on `Terminate` (ADR-0060). Stopping the server here is best effort: the server also exits
// when its stdin closes, which is what keeps it from outliving a host that is killed outright.
exports.deactivate = function () {
	if (client) { client.stop(); }
};
