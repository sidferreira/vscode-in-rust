'use strict';
// A stand-in language server for the LSP fixture: LSP over stdio with Content-Length framing,
// stdlib only, deterministic answers.
//   initialize            -> capabilities and serverInfo (with this process's pid)
//   textDocument/didOpen  -> publishDiagnostics: one Error per line containing "TODO"
//   textDocument/completion -> two items, the second echoing the 0-based position it received
//   shutdown              -> null;  exit -> process exits
// It also exits when stdin closes, so it cannot outlive the client that spawned it.

let buffer = Buffer.alloc(0);

function write(msg) {
	const body = Buffer.from(JSON.stringify(msg), 'utf8');
	process.stdout.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, 'ascii'), body]));
}

function respond(id, result) {
	write({ jsonrpc: '2.0', id, result: result === undefined ? null : result });
}

function diagnosticsFor(text) {
	const out = [];
	text.split('\n').forEach((line, n) => {
		const at = line.indexOf('TODO');
		if (at >= 0) {
			out.push({
				range: { start: { line: n, character: at }, end: { line: n, character: at + 4 } },
				severity: 1,
				source: 'fake-server',
				message: `fake: TODO found`,
			});
		}
	});
	return out;
}

function handle(msg) {
	switch (msg.method) {
		case 'initialize':
			return respond(msg.id, {
				capabilities: { textDocumentSync: 1, completionProvider: { triggerCharacters: ['.'] } },
				serverInfo: { name: 'fake-server', version: '0.0.1', pid: process.pid },
			});
		case 'initialized':
			return;
		case 'textDocument/didOpen': {
			const td = msg.params.textDocument;
			write({
				jsonrpc: '2.0',
				method: 'textDocument/publishDiagnostics',
				params: { uri: td.uri, diagnostics: diagnosticsFor(td.text) },
			});
			return;
		}
		case 'textDocument/completion': {
			const p = msg.params.position;
			return respond(msg.id, {
				isIncomplete: false,
				items: [
					{ label: 'serverAlpha', kind: 3, insertText: 'serverAlpha', detail: 'from fake-server' },
					{ label: `serverPos:${p.line}:${p.character}`, kind: 6 },
				],
			});
		}
		case 'shutdown':
			return respond(msg.id, null);
		case 'exit':
			return process.exit(0);
		default:
			if (msg.id !== undefined) {
				write({ jsonrpc: '2.0', id: msg.id, error: { code: -32601, message: `unknown method ${msg.method}` } });
			}
	}
}

process.stdin.on('data', chunk => {
	buffer = Buffer.concat([buffer, chunk]);
	for (;;) {
		const sep = buffer.indexOf('\r\n\r\n');
		if (sep < 0) { return; }
		const m = /Content-Length:\s*(\d+)/i.exec(buffer.subarray(0, sep).toString('ascii'));
		if (!m) { process.stderr.write('fake-server: missing Content-Length\n'); process.exit(2); }
		const start = sep + 4;
		const end = start + Number(m[1]);
		if (buffer.length < end) { return; }
		const msg = JSON.parse(buffer.subarray(start, end).toString('utf8'));
		buffer = buffer.subarray(end);
		handle(msg);
	}
});
process.stdin.on('end', () => process.exit(0));
process.stdin.on('close', () => process.exit(0));
