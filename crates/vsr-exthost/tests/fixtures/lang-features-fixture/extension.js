// upstream: src/vs/workbench/api/common/extHostLanguageFeatures.ts @ 50e1350fc78d
// A fake language ("fakelang", files *.fk) with every language feature the main side supports.
// Document shape used by the tests:
//   fn alpha(a, b) {
//   <tab>alpha(1, 2)<space>
//   }
//   fn beta() {}
//   // see https://example.com/x
//   struct Point
const vscode = require('vscode');

const SEL = 'fakelang';

function wordAt(doc, pos) {
	const r = doc.getWordRangeAtPosition(pos);
	return r ? { range: r, text: doc.getText(r) } : undefined;
}

function occurrences(doc, word) {
	const out = [];
	const re = new RegExp('\\b' + word + '\\b', 'g');
	for (let line = 0; line < doc.lineCount; line++) {
		const text = doc.lineAt(line).text;
		let m;
		while ((m = re.exec(text))) {
			out.push(new vscode.Range(line, m.index, line, m.index + word.length));
		}
	}
	return out;
}

exports.activate = function (ctx) {
	const l = vscode.languages;
	const sub = d => ctx.subscriptions.push(d);

	sub(l.registerCompletionItemProvider(SEL, {
		provideCompletionItems(doc, pos, token, context) {
			if (token.isCancellationRequested) { return undefined; }
			const alpha = new vscode.CompletionItem('alpha', vscode.CompletionItemKind.Function);
			alpha.insertText = new vscode.SnippetString('alpha(${1:a}, ${2:b})');
			alpha.sortText = '0';
			alpha.commitCharacters = ['('];
			const beta = new vscode.CompletionItem({ label: 'beta', detail: ' (fn)', description: 'fixture' }, vscode.CompletionItemKind.Variable);
			beta.filterText = 'bet';
			beta.range = new vscode.Range(pos.line, 0, pos.line, pos.character);
			beta.additionalTextEdits = [vscode.TextEdit.insert(new vscode.Position(0, 0), '// imported\n')];
			beta.command = { command: 'fixture.noop', title: 'noop', arguments: [1, 'two'] };
			const trigger = context.triggerCharacter || '';
			const gamma = new vscode.CompletionItem('trigger:' + trigger, vscode.CompletionItemKind.Text);
			return new vscode.CompletionList([alpha, beta, gamma], true);
		},
		resolveCompletionItem(item) {
			const label = typeof item.label === 'string' ? item.label : item.label.label;
			item.detail = 'resolved: ' + label;
			item.documentation = new vscode.MarkdownString('**docs** for ' + label);
			return item;
		}
	}, '.'));

	// Same language, different filter shapes: a scheme-and-language filter and one that never matches.
	sub(l.registerHoverProvider({ language: SEL, scheme: 'file' }, {
		provideHover(doc, pos) {
			const w = wordAt(doc, pos);
			return w && new vscode.Hover(new vscode.MarkdownString('hover for `' + w.text + '`'), w.range);
		}
	}));
	sub(l.registerHoverProvider({ language: 'otherlang' }, { provideHover() { return new vscode.Hover('never'); } }));
	sub(l.registerHoverProvider({ language: SEL, scheme: 'untitled' }, { provideHover() { return new vscode.Hover('never'); } }));

	sub(l.registerDocumentSymbolProvider(SEL, {
		provideDocumentSymbols(doc) {
			const out = [];
			for (let line = 0; line < doc.lineCount; line++) {
				const text = doc.lineAt(line).text;
				const m = /^(fn|struct)\s+(\w+)/.exec(text);
				if (!m) { continue; }
				const kind = m[1] === 'fn' ? vscode.SymbolKind.Function : vscode.SymbolKind.Struct;
				const sel = new vscode.Range(line, m[0].length - m[2].length, line, m[0].length);
				const sym = new vscode.DocumentSymbol(m[2], m[1], kind, doc.lineAt(line).range, sel);
				if (m[1] === 'struct') {
					sym.children.push(new vscode.DocumentSymbol('x', 'field', vscode.SymbolKind.Field, sel, sel));
				}
				out.push(sym);
			}
			return out;
		}
	}, { label: 'Fixture' }));

	sub(l.registerDefinitionProvider(SEL, {
		provideDefinition(doc, pos) {
			const w = wordAt(doc, pos);
			if (!w) { return undefined; }
			for (let line = 0; line < doc.lineCount; line++) {
				const text = doc.lineAt(line).text;
				const m = new RegExp('^fn\\s+(' + w.text + ')\\b').exec(text);
				if (m) {
					const target = new vscode.Range(line, 3, line, 3 + w.text.length);
					return [{ originSelectionRange: w.range, targetUri: doc.uri, targetRange: doc.lineAt(line).range, targetSelectionRange: target }];
				}
			}
			return undefined;
		}
	}));
	// A provider that fails: its error must not hide the others' answers.
	sub(l.registerDefinitionProvider('*', { provideDefinition() { throw new Error('boom'); } }));
	sub(l.registerDeclarationProvider(SEL, { provideDeclaration(doc, pos) { return new vscode.Location(doc.uri, new vscode.Position(0, 0)); } }));
	sub(l.registerImplementationProvider(SEL, { provideImplementation(doc, pos) { return new vscode.Location(doc.uri, new vscode.Position(3, 0)); } }));
	sub(l.registerTypeDefinitionProvider(SEL, { provideTypeDefinition(doc, pos) { return new vscode.Location(doc.uri, new vscode.Position(5, 0)); } }));

	sub(l.registerReferenceProvider(SEL, {
		provideReferences(doc, pos, context) {
			const w = wordAt(doc, pos);
			if (!w) { return undefined; }
			const all = occurrences(doc, w.text).map(r => new vscode.Location(doc.uri, r));
			return context.includeDeclaration ? all : all.filter(loc => doc.lineAt(loc.range.start.line).text.indexOf('fn ') !== 0);
		}
	}));

	sub(l.registerDocumentHighlightProvider(SEL, {
		provideDocumentHighlights(doc, pos) {
			const w = wordAt(doc, pos);
			return w && occurrences(doc, w.text).map((r, i) => new vscode.DocumentHighlight(r, i === 0 ? vscode.DocumentHighlightKind.Write : vscode.DocumentHighlightKind.Read));
		}
	}));

	sub(l.registerCodeLensProvider(SEL, {
		provideCodeLenses(doc) {
			const out = [];
			for (let line = 0; line < doc.lineCount; line++) {
				if (/^fn\s/.test(doc.lineAt(line).text)) { out.push(new vscode.CodeLens(new vscode.Range(line, 0, line, 2))); }
			}
			return out;
		},
		resolveCodeLens(lens) {
			lens.command = { title: 'lens@' + lens.range.start.line, command: 'fixture.noop', arguments: [lens.range.start.line] };
			return lens;
		}
	}));

	sub(l.registerInlayHintsProvider(SEL, {
		provideInlayHints(doc, range) {
			const out = [];
			for (let line = range.start.line; line <= range.end.line && line < doc.lineCount; line++) {
				const i = doc.lineAt(line).text.indexOf('alpha(1');
				if (i >= 0) {
					const h = new vscode.InlayHint(new vscode.Position(line, i + 6), 'a:', vscode.InlayHintKind.Parameter);
					h.paddingRight = true;
					out.push(h);
				}
			}
			return out;
		},
		resolveInlayHint(hint) {
			hint.tooltip = 'resolved hint';
			return hint;
		}
	}));

	sub(l.registerSignatureHelpProvider(SEL, {
		provideSignatureHelp(doc, pos, token, context) {
			const help = new vscode.SignatureHelp();
			const sig = new vscode.SignatureInformation('alpha(a, b)', new vscode.MarkdownString('Calls alpha'));
			sig.parameters = [new vscode.ParameterInformation('a', 'first'), new vscode.ParameterInformation([9, 10], 'second')];
			help.signatures = [sig];
			help.activeSignature = 0;
			help.activeParameter = context.triggerCharacter === ',' ? 1 : 0;
			return help;
		}
	}, '(', ','));

	sub(l.registerCodeActionsProvider(SEL, {
		provideCodeActions(doc, range, context) {
			const fix = new vscode.CodeAction('Fix it', vscode.CodeActionKind.QuickFix);
			fix.isPreferred = true;
			fix.edit = new vscode.WorkspaceEdit();
			fix.edit.replace(doc.uri, new vscode.Range(0, 3, 0, 8), 'omega');
			const lazy = new vscode.CodeAction('Extract lazily', vscode.CodeActionKind.RefactorExtract);
			lazy.data = 'lazy';
			const off = new vscode.CodeAction('Not now', vscode.CodeActionKind.Refactor);
			off.disabled = { reason: 'nothing selected' };
			return [fix, lazy, off];
		},
		resolveCodeAction(action) {
			action.edit = new vscode.WorkspaceEdit();
			action.edit.insert(vscode.Uri.file('/elsewhere.fk'), new vscode.Position(0, 0), 'lazy edit');
			return action;
		}
	}, { providedCodeActionKinds: [vscode.CodeActionKind.QuickFix, vscode.CodeActionKind.Refactor] }));

	sub(l.registerRenameProvider(SEL, {
		prepareRename(doc, pos) {
			const w = wordAt(doc, pos);
			if (!w) { throw new Error('nothing to rename here'); }
			return { range: w.range, placeholder: w.text };
		},
		provideRenameEdits(doc, pos, newName) {
			const w = wordAt(doc, pos);
			const edit = new vscode.WorkspaceEdit();
			for (const r of occurrences(doc, w.text)) { edit.replace(doc.uri, r, newName); }
			return edit;
		}
	}));

	sub(l.registerFoldingRangeProvider(SEL, {
		provideFoldingRanges(doc) {
			return [new vscode.FoldingRange(0, 2, vscode.FoldingRangeKind.Region), new vscode.FoldingRange(4, 5, vscode.FoldingRangeKind.Comment)];
		}
	}));

	sub(l.registerDocumentLinkProvider(SEL, {
		provideDocumentLinks(doc) {
			const out = [];
			for (let line = 0; line < doc.lineCount; line++) {
				const text = doc.lineAt(line).text;
				const m = /https?:\/\/\S+/.exec(text);
				if (m) { out.push(new vscode.DocumentLink(new vscode.Range(line, m.index, line, m.index + m[0].length), vscode.Uri.parse(m[0]))); }
			}
			const unresolved = new vscode.DocumentLink(new vscode.Range(0, 0, 0, 2));
			out.push(unresolved);
			return out;
		},
		resolveDocumentLink(link) {
			link.target = vscode.Uri.parse('https://example.com/resolved');
			link.tooltip = 'resolved link';
			return link;
		}
	}));

	sub(l.registerDocumentFormattingEditProvider(SEL, {
		provideDocumentFormattingEdits(doc, options) {
			const edits = [];
			const indent = options.insertSpaces ? ' '.repeat(options.tabSize) : '\t';
			for (let line = 0; line < doc.lineCount; line++) {
				const text = doc.lineAt(line).text;
				const fixed = text.replace(/^\t/, indent).replace(/[ \t]+$/, '');
				if (fixed !== text) { edits.push(vscode.TextEdit.replace(doc.lineAt(line).range, fixed)); }
			}
			return edits;
		}
	}));
	sub(l.registerDocumentRangeFormattingEditProvider(SEL, {
		provideDocumentRangeFormattingEdits(doc, range) {
			return [vscode.TextEdit.replace(range, doc.getText(range).toUpperCase())];
		}
	}));
	sub(l.registerOnTypeFormattingEditProvider(SEL, {
		provideOnTypeFormattingEdits(doc, pos, ch) {
			return [vscode.TextEdit.insert(pos, '/*' + ch + '*/')];
		}
	}, ';'));

	sub(vscode.commands.registerCommand('fixture.noop', () => undefined));
};
