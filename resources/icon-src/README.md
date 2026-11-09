# App icon sources

`../darwin/rscode.icns` = Code-OSS icon + Ferris, composed by `scripts/make-app-icon.swift`:

```sh
swift scripts/make-app-icon.swift resources/icon-src/code-oss-1024.png \
  resources/icon-src/ferris-rustacean-flat-happy.png out-1024.png
# then: sips each size into an .iconset and `iconutil -c icns`
```

| file | source | license |
|---|---|---|
| `code-oss-1024.png` | `vscode-original/resources/darwin/code.icns` (1024px) @ 50e1350fc78d | MIT, © Microsoft Corporation. This is the open-source Code-OSS icon, not the trademarked Visual Studio Code product logo. |
| `ferris-rustacean-flat-happy.png` | https://rustacean.net/assets/rustacean-flat-happy.png | CC0 (public domain), Ferris by Karen Rustad Tölva |
