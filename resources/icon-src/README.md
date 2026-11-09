# App icon sources

`../darwin/rscode.icns` is original artwork drawn by `scripts/make-app-icon.swift`: a copper
"ribbon" mark (chevron folding into a panel) on a light macOS tile, with Ferris the crab.
No Microsoft logo or Code-OSS artwork is used.

```sh
swift scripts/make-app-icon.swift resources/icon-src/ferris-rustacean-flat-happy.png out-1024.png
# then sips each size (16..512, @1x/@2x) into rscode.iconset and: iconutil -c icns rscode.iconset
```

| file | source | license |
|---|---|---|
| `ferris-rustacean-flat-happy.png` | https://rustacean.net/assets/rustacean-flat-happy.png | CC0 (public domain), Ferris by Karen Rustad Tölva |
