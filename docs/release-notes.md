# Jott 0.0.11 — a screen that says what the backend has been doing

Press **Ctrl+Shift+D**. A drawer opens at the bottom of the window with this run's whole conversation with np:
which np file is running, every request and how long it took, np's own error sentences, what is queued, how many
notes are indexed and embedded, and where the log lives — with *Copy details* and *Open log folder* buttons.

Until now that information existed only as `%APPDATA%\jott\logs\jott.jsonl`, which meant reporting a problem took
finding a path, opening Notepad, and reading JSON at eleven o'clock. The drawer shows the same records — the ones
the journal writes — so a screenshot and a log can no longer disagree about the same scan.

## New since 0.0.10

- **`Ctrl+Shift+D`** opens and closes the drawer; **Esc** closes it too. It is part of the main window rather than a
  second one, because the thing you are looking at is usually the note a new window would cover.
- **Rows arrive while you watch.** A save that stalls shows the request land and nothing follow it — which is a
  different diagnosis from no request at all, and says whether to look at Jott or at np.
- **np's sentences are kept whole.** The row shortens for display; the tooltip and *Copy details* carry every word
  np wrote, unedited. "np said: the index is locked by another process" is the answer to a question that used to
  take three messages.
- **Connections are named, not guessed.** `writer connected · reader not started` is normal until you search;
  `gone` means a child died. On a machine with two np installs, the path on the second line ends the argument.
- **Consent and fetching, visible.** What the registry remembered, and whether np's config currently lets it pull
  a model on its own — the setting that decides whether a 300 MB download starts.
- **Nothing you typed is in there, ever.** The rows carry method, timing, outcome, np's status words. The key set
  is closed and pinned by tests on both sides: a future field holding "the note involved" fails CI rather than
  quietly shipping your prose to a screenshot.

Opening the drawer asks np nothing and starts nothing, so it is safe to press on a machine where np is missing or
unwell — which is usually exactly when you want it.

## Also in this build, since 0.0.10

Faster folder listings (a 2,000-note vault relists in about 3 ms instead of ~35 ms), a caret you can find at
Windows display scaling, search terms marked in results and in the opened note, and markdown styled in place
instead of raw syntax — no side-by-side preview, so typing keeps its full width.

## Earlier, if you are jumping in

- **0.0.9** — *Index now* covers the whole folder; embeddings stay off until you answer, and Jott writes
  `auto_fetch = false` so np cannot download a model behind the prompt.
- **0.0.8** — closing a folder or quitting lets go of every np child; startup's np is reused rather than restarted.
- **0.0.7** — no black console windows from np, tree markers agree with the status line, and a new folder says
  "N notes are not indexed yet" instead of nothing.
- **0.0.6** — the bridge to np actually connects: Jott starts `np serve` (bare `np` prints usage and exits) and
  folds what np said on stderr into the error you see.
- **0.0.5** — the About panel: both versions, which np file this run picked, where Jott keeps its files.
- **0.0.4** — dark mode stopped shipping invisible text; the window opens with a visible skeleton.
- **0.0.3** — np ships inside the installer, so installing Jott installs the backend.

Install `jott-windows-x64.msi`, then work through [docs/testing-on-windows.md](testing-on-windows.md).
