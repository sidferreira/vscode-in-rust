// upstream: src/vs/workbench/api/common/extHostFileSystemConsumer.ts @ 50e1350fc78d
// Every file-scheme operation of vscode.workspace.fs, each in its own try, so the report lists what
// each step returned or threw. The node host runs stat, readFile, writeFile, mkdir and plain delete
// in the host itself (its disk provider); rename, copy and trash deletes are forwarded to the main side. `code` is FileSystemError.code (FileNotFound, FileExists, ...).
// fixture.fsRun(dir, outside): `dir` is the workspace folder; `outside` is a folder the host may
// not destroy in.
const vscode = require('vscode');
const path = require('path');

const fsApi = vscode.workspace.fs;
const enc = new TextEncoder();
const dec = new TextDecoder();

async function attempt(report, label, fn) {
	try {
		report[label] = { ok: true, value: await fn() };
	} catch (e) {
		report[label] = { ok: false, code: e.code, name: e.name, message: String(e.message) };
	}
}

exports.activate = function (ctx) {
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.fsRun', (dir, outside) => run(dir, outside)));
};

async function run(dir, outside) {
	const u = (...parts) => vscode.Uri.file(path.join(dir, ...parts));
	const out = {};
	await attempt(out, 'writeNew', async () => { await fsApi.writeFile(u('a.txt'), enc.encode('hello')); return 'ok'; });
	await attempt(out, 'readFile', async () => dec.decode(await fsApi.readFile(u('a.txt'))));
	await attempt(out, 'stat', async () => { const s = await fsApi.stat(u('a.txt')); return { type: s.type, size: s.size }; });
	await attempt(out, 'statMissing', () => fsApi.stat(u('nope.txt')));
	await attempt(out, 'readDirectory', async () => (await fsApi.readDirectory(u())));
	await attempt(out, 'createDirectory', async () => { await fsApi.createDirectory(u('sub', 'deep')); return 'ok'; });
	await attempt(out, 'statFolder', async () => (await fsApi.stat(u('sub'))).type);
	await attempt(out, 'readFileOfFolder', () => fsApi.readFile(u('sub')));
	await attempt(out, 'readDirOfFile', () => fsApi.readDirectory(u('a.txt')));
	await attempt(out, 'writeCreatesParents', async () => { await fsApi.writeFile(u('sub', 'x', 'b.txt'), enc.encode('bee')); return 'ok'; });
	await attempt(out, 'writeOverFolder', () => fsApi.writeFile(u('sub'), enc.encode('x')));
	await attempt(out, 'renameNoOverwrite', () => fsApi.rename(u('a.txt'), u('sub', 'x', 'b.txt')));
	await attempt(out, 'rename', () => fsApi.rename(u('a.txt'), u('c.txt')));
	await attempt(out, 'renameOverwrite', async () => {
		await fsApi.writeFile(u('d.txt'), enc.encode('d'));
		await fsApi.rename(u('c.txt'), u('d.txt'), { overwrite: true });
		return dec.decode(await fsApi.readFile(u('d.txt')));
	});
	await attempt(out, 'copyFolder', () => fsApi.copy(u('sub'), u('sub2')));
	await attempt(out, 'copyFileOnce', () => fsApi.copy(u('d.txt'), u('e.txt')));
	await attempt(out, 'copyFileAgain', () => fsApi.copy(u('d.txt'), u('e.txt')));
	await attempt(out, 'deleteMissing', () => fsApi.delete(u('missing')));
	await attempt(out, 'deleteNonEmptyFolder', () => fsApi.delete(u('sub')));
	await attempt(out, 'deleteRecursive', () => fsApi.delete(u('sub'), { recursive: true }));
	await attempt(out, 'deleteTrashInside', () => fsApi.delete(u('d.txt'), { useTrash: true }));
	await attempt(out, 'writeForOutside', () => fsApi.writeFile(u('z.txt'), enc.encode('z')));
	await attempt(out, 'overwriteOutsideFolder', () => fsApi.rename(u('z.txt'), vscode.Uri.file(outside), { overwrite: true }));
	await attempt(out, 'outsideReadAfter', async () => dec.decode(await fsApi.readFile(vscode.Uri.file(path.join(outside, 'keep.txt')))));
	await attempt(out, 'outsideTrash', () => fsApi.delete(vscode.Uri.file(path.join(outside, 'keep.txt')), { useTrash: true }));
	await attempt(out, 'otherScheme', () => fsApi.readFile(vscode.Uri.parse('untitled:Untitled-1')));
	return out;
}
