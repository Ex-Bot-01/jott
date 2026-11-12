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
   program is enough. Check `%APPDATA%\jott\logs\jott.jsonl` if you are curious which binary answered; it
   names the version. If you would rather Jott used an np you built or installed yourself, set `NP_PATH` to
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

## 3. The question only Windows can answer: write-lock contention

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

## 4. What to send back

One file, plus anything you noticed in words:

```
%APPDATA%\jott\logs\jott.jsonl
```

That is the whole log: one line per request Jott made to np — the method, which vault, how long it took, and
the outcome (`applied`, `locked`, `offline`, `disabled`, `refused`, `fault`) — with np's own error sentence on
a fault. It rotates at 2 MiB and keeps older copies behind it (`jott.jsonl.1`, `jott.jsonl.2`). **It never
contains the contents of your notes**; if you would rather look first, open it in Notepad and read it — each
line is one JSON object with seven possible keys and nothing else.

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
