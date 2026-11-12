# Building Jott from source on Windows

For anyone who wants the app without waiting on CI — or who is the person CI runs for. The published
installer comes from `.github/workflows/build.yml`; this is the same recipe, run by hand.

## What you need

| Piece | Why | Get it |
| --- | --- | --- |
| Git | the source | <https://git-scm.com/download/win> |
| Node 22 | the window (Vite + Svelte) | <https://nodejs.org> |
| Rust, **MSVC** toolchain | the app | <https://rustup.rs/> — accept the default host triple, it picks MSVC |
| Visual Studio Build Tools, *Desktop development with C++* | linking, and the Windows resource compiler | <https://visualstudio.microsoft.com/visual-cpp-build-tools/> |
| WebView2 runtime | the webview itself | already on Windows 11; on 10 it comes via [Evergreen installer](https://developer.microsoft.com/microsoft-edge/webview2/) |

np is not needed to build. Jott finds it at run time or says how to install it ([`INSTALL.md`](../INSTALL.md) §2).

## Build

```powershell
git clone https://github.com/Ex-Bot-01/jott.git
cd jott
npm ci
cd apps\shell
npx tauri build --bundles msi
```

`tauri build` runs the front-end build itself (`beforeBuildCommand` in `apps/shell/tauri.conf.json`), so there
is no separate `npm run build` step. The first run compiles about 400 crates and takes ten to twenty minutes;
after that it is incremental.

The installer lands at:

```
apps\shell\target\release\bundle\msi\Jott_<version>_x64_en-US.msi
```

It is unsigned, so Windows will warn about an unknown publisher before it runs. To put it on a release page:

```powershell
cd C:\path\to\jott
gh release create v0.0.2 --title "Jott v0.0.2" --notes-file docs\release-notes.md `
  apps\shell\target\release\bundle\msi\Jott_0.0.2_x64_en-US.msi
```

## Checking the Rust side without building a bundle

The core crate is where the behaviour lives and it needs no GUI libraries:

```powershell
cd crates\jott-core
cargo test
```

On Linux, the Windows build can be type-checked without a Windows machine — which is how most of this code was
written — because Tauri's Windows dependencies are pure Rust:

```bash
rustup target add x86_64-pc-windows-msvc
cd apps/shell && cargo check --target x86_64-pc-windows-msvc
```

That last step needs a Windows resource compiler (`llvm-rc`) to get past `tauri-build`'s build script. On a
machine where you cannot install one, a stub that writes the `.res` file it is asked for and exits 0 is enough
to reach the type checker — but nothing less than a real Windows build counts as a build: the resource
compiler is also what embeds the icon and the manifest, and `cargo check` never links.
