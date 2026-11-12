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

## Fetching np first

The installer ships np inside it, so a person who installs Jott can index and search without downloading
anything else. The binary is not in git — it is fetched from np's releases, checked against the hash pinned in
[`packaging/np-pin.json`](../packaging/np-pin.json), and unpacked into `apps\shell\np\`, which the bundler
copies into `$RESOURCES\np`:

```powershell
.\scripts\fetch-np.ps1        # downloads, verifies the sha256, unpacks np.exe
npm ci
cd apps\shell
npx tauri build --bundles msi
```

Skipping the fetch does not break the build; the bundle simply has no `np` folder to copy, and Jott falls back
to `$NP_PATH`, then `PATH`, and says plainly what it found. [Building on Linux](#checking-the-rust-side-without-building-a-bundle)
uses `./scripts/fetch-np.sh`, which does the same thing with `sha256sum`.

## Build

```powershell
git clone https://github.com/Ex-Bot-01/jott.git
cd jott
.\scripts\fetch-np.ps1
npm ci
cd apps\shell
npx tauri build --bundles msi
```

`tauri build` runs the front-end build itself (`beforeBuildCommand` in `apps/shell/tauri.conf.json`), so there
is no separate `npm run build` step. np itself is never built from source here: Jott's ground rules keep
[note-processor](https://github.com/Ex-Bot-01/note-processor) a separate project, and this only ever copies a
published release. The first run compiles about 400 crates and takes ten to twenty minutes;
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
