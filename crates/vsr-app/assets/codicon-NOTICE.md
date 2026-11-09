# codicon.ttf: modified copy

Source: `@vscode/codicons` 0.0.46-49 (`dist/codicon.ttf`), © Microsoft Corporation,
licensed CC BY 4.0 (`codicon-LICENSE.txt`), https://github.com/microsoft/vscode-codicons

**Change (2026-11-09):** added a cmap mapping U+006D ('m') → the existing `blank` glyph
(U+EC03) in every Unicode cmap subtable, made with `scripts/patch-codicon-font.py`.
No glyph outlines were changed. Reason: gpui's macOS font loader skips fonts that have no
'm' glyph, which made every icon render as a missing-glyph box (ADR-0021).
