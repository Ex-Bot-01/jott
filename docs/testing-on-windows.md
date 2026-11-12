# Testing Jott on Windows

Jott is built for Windows first (spec §19): it is where the app will be used, and it is the only place the
things that matter can be seen — WebView2's text rendering, the write lock np holds on a workspace, and
whether an unsaved buffer ever comes back when it should not.

Everything below runs on a normal Windows machine. Nothing here needs this repository.

## 1. Install

1. **Jott** — go to <https://github.com/Ex-Bot-01/jott/releases>, open the newest release, and download
   `jott-windows-x64.msi`. Double-click it; Windows may warn that the publisher is unknown, because this
   build is unsigned until the signing story in spec §20 exists — *More info → Run anyway* is the expected
   path. (If you were sent a link to a workflow run instead of a release, that is an artifact: it needs a
   GitHub account with access to this repository and it disappears after 30 days. A release needs neither.)
   No download at all — because CI has not produced one yet, or you want a build newer than the last tag —
   then [`docs/building-on-windows.md`](building-on-windows.md) is four commands.
2. **np comes with it.** The installer carries the pinned np build inside it — that is why installing one
   program is enough. The bottom-left of the window says which np version is answering. If you would rather
   Jott used an np you built or installed yourself, set `NP_PATH` to
   it: an override you set beats the copy we ship, deliberately (§14).
3. **Check what you downloaded**, if you want the certainty rather than the assumption. In PowerShell, in the
   folder you downloaded to, next to `SHA256SUMS.txt` from the same release:

   ```powershell
   (Get-FileHash .\jott-windows-x64.msi).Hash.ToLower()
   Select-String jott-windows-x64.msi .\SHA256SUMS.txt
   ```

   The two strings should match. If they do not, the file did not arrive as CI built it — say so and download
   again.
If something is wrong with the bundled copy — deleted by an antivirus, or you installed from before np started
shipping inside the installer — Jott still opens, everything except search and embedding works, and the
bottom-left of the window says what is missing and how to install it. That state is worth a look too: it is what
a broken install looks like. To install np by hand instead, follow
[`INSTALL.md` §2, "Windows (PowerShell)"](../INSTALL.md#windows-powershell--same-procedure) and point `NP_PATH`
at the result.

## 2. The five things to try

Opening a folder does not index it by itself: indexing writes `INDEX.md` into your vault, so it stays something
you ask for. The sidebar says *N notes are not indexed yet* with an **Index now** button — press it once, or save
any note, and the markers start moving.

When you open a folder for the first time, Jott does not index it by itself — indexing writes `INDEX.md` into
your vault, so it stays a thing you ask for. Say it once: **Index now** in the sidebar, or save any note.

Take your time with each and note anything that looks wrong, in words — exact wording is deliberate, so if a
sentence reads oddly, say so.

1. **First run.** *Open a folder*, pick a notes folder with a few markdown files in it. The tree should list
   them, each with a `●` beside it (something is owed to the index). An `INDEX.md` written by np appears
   greyed and italic — clicking it does nothing, which is correct. A folder that already carries np's
   `INDEX.md` gets one sentence about being inside another vault.
2. **Save, then watch the embedding.** Type a sentence into a note and stop typing: about two seconds later
   it saves on its own (the small line under the editor says `saved`). The status line should then say
   something like `indexing…`, then `embedding 3/7 chunks` if np is fetching or embedding a model, then
   `embedded`. Note how long the whole thing takes from your last keystroke to the marker turning `·`. If
   np's config has no embedding model, you should be *asked* once — with the model's name and the folder it
   would download into — rather than have a download start. Saying no keeps keyword search working.
3. **Rename.** Select a note, press `rename`, give it a new name in the same folder. It must refuse to
   overwrite an existing name (and say so), and if another note links to it with `[[like this]]`, the notice
   should count how many notes refer to it. Nothing rewrites those links yet — that is a later plan — so a
   rename of a linked note is expected to leave the referring text alone.
4. **A file changed by another editor.** Open a note in Jott, then change the *same* file in Notepad or VS
   Code and save it there:
   - with no unsaved typing in Jott, the editor should quietly show the new text;
   - with unsaved typing (type something and do not save), Jott must say the file changed outside and offer
     *Reload the file* or *Keep typing*. Choose each once, in separate runs. It must never overwrite either
     side on its own.
5. **Closing mid-embed.** Start an embed (the `[embed]` button in the status line), then close the window
   while it says `embedding …`. Reopen: Jott should come back promptly — it waits a few seconds at most, and
   never for a model download. Whatever was saved before closing should be indexed on the next pass rather
   than lost.

Two more, if you have the patience:

- **Delete something** and check where it went. The notice says which: the system trash, or — on a machine
  where Windows offers no trash for that path — `%APPDATA%\jott\trash\`. A file in there is retrievable.
- **Type into a note and close the laptop lid** (or end the process from Task Manager) before the two-second
  autosave fires. On reopening, Jott should offer the text it had mirrored: "You had unsaved text from an
  earlier session", with *Use it* / *Discard it*. **This one is the test most worth reporting on**, in either
  direction — including "it offered me text I had deliberately deleted", which is a bug, not a nuisance.

## 3. One thing worth watching: np processes

Jott starts np when it opens and keeps one running; a second starts the first time you search and stays until you
switch folders or quit. When you close a folder or leave the app, they should go away within a second or two. If
Task Manager ever shows a `np.exe` still running after Jott has closed, that is a bug worth telling me about — on
Windows it means your vault's index stays locked for the next np.

## 4. The question only Windows can answer: write-lock contention

np takes a lock on the workspace while it indexes, and on Windows that lock is a file others cannot open.
Jott must treat "someone else is indexing" as a state, not an error.

1. Open your vault in Jott and press `[embed]`.
2. While the status line says `embedding …`, open PowerShell in the same folder and run:

   ```powershell
   np scan .
   ```

   (An absolute path works the same way if you are not standing in the folder.)

3. What should happen: whichever of the two is waiting reports **`index behind · retrying in 2 s`** (then 5,
   15 and a minute at most — the ladder §11 names), the window stays responsive, and within a couple of
   minutes the markers settle. What must not happen: an error dialog, a frozen window, or a note that stops
   saving.

Then do it the other way round — `np scan .` running first, and a save in Jott while it works.

## 5. What to send back

One file, plus anything you noticed in words:

```
%APPDATA%\jott\logs\jott.jsonl
```

Send the About panel's *Copy details* text with it too — it names the np binary, which is the first thing I ask.

That is the whole log: one line per request Jott made to np — the method, which vault, how long it took, and
the outcome (`applied`, `locked`, `offline`, `disabled`, `refused`, `fault`) — with np's own error sentence on
a fault. It rotates at 2 MiB and keeps older copies behind it (`jott.jsonl.1`, `jott.jsonl.2`). **It never
contains the contents of your notes**; if you would rather look first, open it in Notepad and read it — each
line is one JSON object with seven possible keys and nothing else.

## If the window shows nothing

Press **Ctrl+Shift+I**. Devtools are left on in these builds, and the Console tab is the difference between
"everything is white" and "`invoke` rejected `list_vault` with `npGone`". Send what it says; a screenshot of a
blank window tells me the window works and nothing else.

**First, open *About* in the sidebar header.** It says which np this run is talking to — version, and the full
path of the binary — plus where Jott's data and log live, and **Copy details** puts all of it on your clipboard
as a block you can paste straight into a message. That block answers most of what I would otherwise ask for.

Three things worth knowing before you file it:

- **A window the colour of paper, with no text at all**, on a Windows set to dark mode was 0.0.3's bug — white
  ink on near-white background. Fixed in 0.0.4; if you are on 0.0.4 and see it, that is a new bug and I want
  the console output.
- **A black console window titled `np.exe`.** Windows gives a console to any console program a GUI starts, and
  np is one. Fixed in 0.0.7 (`CREATE_NO_WINDOW`). On 0.0.6 that window was the helper doing its job quietly — nothing
  had gone wrong, but it looked like something had.
- **Markers lagging behind the status line.** In 0.0.6 the tree kept the stamps from when it was listed, so
  "completed" could sit above dots saying *waiting for the index*. 0.0.7 re-lists after a pass finishes.
- **"np ended before answering"** was Jott's own bug, not yours: it started np without telling it to `serve`,
  so np printed its usage and left. Fixed in 0.0.6, where the message also carries whatever np said on stderr —
  if you see it again on 0.0.6, that quoted text is the thing I need.
- **"Jott is starting…" that never goes away** means the bundle loaded but the app did not mount. That text is
  drawn by plain HTML precisely so this case has something to show.
- **A status line naming no np version** means Jott could not find np. The installer carries it, so on a normal
  install that is a bug — unless `NP_PATH` points somewhere that no longer exists, which wins on purpose.

## Uninstalling leaves your files alone

Uninstalling Jott removes the program and keeps everything under `%APPDATA%\jott` — on purpose. That folder can
hold a mirrored copy of text you had not saved, and notes Jott moved to its own trash folder because Windows
offered none; an uninstaller that deleted either would be destroying writing. np keeps its own
`%APPDATA%\note-processor` (its index and any model it downloaded) for the same reason.

To clear them once you are sure you want nothing back:

```powershell
Remove-Item -Recurse -Force $env:APPDATA\jott
Remove-Item -Recurse -Force $env:APPDATA\note-processor   # also removes np's index and models
```

## Also useful if something lost data, and only if it did:

| What | Where |
| --- | --- |
| The tree's per-file state | `%APPDATA%\jott\state\<a hash of the vault's path>\state.json` |
| Mirrored unsaved buffers (§18.13) | `%APPDATA%\jott\buffers\` |
| Files Jott trashed when Windows offered no trash | `%APPDATA%\jott\trash\` |
| Registered vaults and the consent decision | `%APPDATA%\jott\vaults.json` |

A screenshot of a status line that confused you is worth as much as the log.
