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
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.findFiles', () => findFiles()));
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.watchArm', () => watchArm()));
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.watchMove', () => watchMove()));
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.watchCollect', () => watchCollect()));
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.watchDisarm', () => watchDisarm()));
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.watchArmString', () => watchArmString()));
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.watchCollectString', () => watchCollectString()));
	ctx.subscriptions.push(vscode.commands.registerCommand('fixture.watchDisarmString', () => watchDisarmString()));
};

// Watchers over the workspace. The events are recorded in watchState until watchCollect takes them.
const watchState = { events: [], disposables: [], stringEvents: [], stringDisposables: [] };

function watchArm() {
	const root = vscode.workspace.workspaceFolders[0].uri;
	const watcher = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(root, '**/*'));
	const record = kind => uri => watchState.events.push(kind + ' ' + path.relative(root.fsPath, uri.fsPath).split(path.sep).join('/'));
	watchState.disposables = [watcher, watcher.onDidCreate(record('created')), watcher.onDidChange(record('changed')), watcher.onDidDelete(record('deleted'))];
	return 'armed';
}

// A string-pattern watcher: upstream never sends it to $watch; the workspace folder delivers its events.
function watchArmString() {
	const root = vscode.workspace.workspaceFolders[0].uri;
	const watcher = vscode.workspace.createFileSystemWatcher('**/*');
	const record = kind => uri => watchState.stringEvents.push(kind + ' ' + path.relative(root.fsPath, uri.fsPath).split(path.sep).join('/'));
	watchState.stringDisposables = [watcher, watcher.onDidCreate(record('created')), watcher.onDidChange(record('changed')), watcher.onDidDelete(record('deleted'))];
	return 'armed';
}

function watchCollectString() {
	const events = watchState.stringEvents.slice();
	watchState.stringEvents.length = 0;
	return events;
}

function watchDisarmString() {
	for (const d of watchState.stringDisposables) { d.dispose(); }
	watchState.stringDisposables = [];
	return 'disarmed';
}

// Forwarded operations (rename, trash delete) and a native write: only the forwarded ones are seen.
async function watchMove() {
	const ws = vscode.workspace.workspaceFolders[0].uri.fsPath;
	const u = p => vscode.Uri.file(path.join(ws, p));
	await fsApi.writeFile(u('move-me.txt'), enc.encode('m'));
	await fsApi.rename(u('move-me.txt'), u('moved.txt'));
	await fsApi.delete(u('moved.txt'), { useTrash: true });
	await new Promise(resolve => setTimeout(resolve, 300));
	return 'moved';
}

function watchCollect() {
	const events = watchState.events.slice();
	watchState.events.length = 0;
	return events;
}

function watchDisarm() {
	for (const d of watchState.disposables) { d.dispose(); }
	watchState.disposables = [];
	return 'disarmed';
}

// workspace.findFiles: the include and exclude arguments, the result limit, a RelativePattern, and
// the user's files.exclude. Results are paths relative to the workspace, sorted.
async function findFiles() {
	const dir = vscode.workspace.workspaceFolders[0].uri.fsPath;
	const rel = uris => uris.map(u => path.relative(dir, u.fsPath).split(path.sep).join('/')).sort();
	const out = {};
	out.all = rel(await vscode.workspace.findFiles('**/*.txt'));
	out.excluded = rel(await vscode.workspace.findFiles('**/*.txt', '**/skip/**'));
	out.limited = (await vscode.workspace.findFiles('**/*.txt', undefined, 2)).length;
	out.relative = rel(await vscode.workspace.findFiles(new vscode.RelativePattern(vscode.Uri.file(path.join(dir, 'sub')), '*.txt')));
	return out;
}

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
