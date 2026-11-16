<script lang="ts">
  // The note itself: CodeMirror 6 with markdown tinting, line wrap on, no minimap (§9), and every
  // save-or-not decision taken by `saveState()` rather than by anything in this file (§11, §14).

  import { onDestroy, untrack } from 'svelte'
  import { Decoration, EditorView, keymap, type ViewUpdate } from '@codemirror/view'
  import { Compartment, EditorState, RangeSet, StateField, type Extension } from '@codemirror/state'
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
  import { markdown } from '@codemirror/lang-markdown'
  import { bracketMatching, syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
  import { markdownStyle } from './markdownStyle'
  import { currentScheme, watchScheme, type Scheme } from './darkMode'
  import { matchRanges } from './highlight'
  import { saveState, wantsSave, type SavePhase } from './saveState'
  import { hashText } from './hashText'
  import type { NoteContent, WriteResult } from './ipc'

  /** Half a second of typing is the most a crash may take (§18.13), so the mirror does not wait for the save tick. */
  const MIRROR_AFTER_MS = 500
  /** §9's autosave: two seconds after the last keystroke, plus blur and unload. */
  const AUTOSAVE_AFTER_MS = 2_000

  let {
    rel,
    note,
    external,
    saveSignal = 0,
    kept = false,
    highlight = '',
    highlightPrefix = false,
    readonly = false,
    onphase,
    onsave,
    onmirror,
    onusemirror,
    ondiscardmirror,
    onreload,
    onkeep,
  }: {
    rel: string | null
    /** What the file said when this buffer was filled — and what a reload replaces the buffer with. */
    note: NoteContent | null
    /** Bumped by the parent to put text in from outside: a reload, or an accepted mirror (§14, §18.13). */
    external: { text: string; token: number } | null
    /** Bumped by the parent when someone presses Ctrl+S with focus outside the editor. The editor itself binds
     *  Mod-s while it has focus; a window-level binding cannot reach into the buffer from out there, and copying
     *  the text up "so the app can save it" would put a second copy of somebody's words in memory. So the parent
     *  asks, and this saves what is actually on screen. */
    saveSignal?: number
    /** "Keep typing" was pressed for this note: the banner has been answered, so it stands down. */
    kept?: boolean
    /** The words that made this note show up in a search, marked in the text and scrolled to. Empty means
     * nobody searched, or this note was opened from the tree rather than from a hit. */
    highlight?: string
    /** Whether the search that produced these marks widened words (note-processor#6). The marks in a result row and
     *  the marks in the note it opens are answered by the same module with the same flag, or they disagree about what
     *  matched — which is the §18.11 complaint in a new place. */
    highlightPrefix?: boolean
    /** Show it, do not offer to change it: note-processor's own catalogue (§9). Core also refuses the write, so this
     *  is not the only thing standing between someone and a damaged index — it is the thing that stops the window
     *  from looking like an editor when it isn't one. */
    readonly?: boolean
    /** The parent needs the phase to decide what an outside change means (§14): a clean buffer reloads,
     * a dirty one asks. It is reported, never recomputed there. */
    onphase?: (phase: SavePhase) => void
    onsave: (rel: string, text: string) => Promise<WriteResult>
    onmirror: (rel: string, text: string) => void
    onusemirror: () => void
    ondiscardmirror: () => void
    onreload: () => void
    onkeep: () => void
  } = $props()

  /** Search marks live in their own compartment: changing the query reconfigures the highlighting and nothing
   * else, so it cannot disturb the buffer, the caret or the undo history (§4's rule about Ctrl+S applies
   * equally to a keystroke in the search box). */
  const highlighter = new Compartment()
  const themer = new Compartment()
  /** Editable or display, per file. A compartment because switching notes must not rebuild the buffer: opening a
   *  read-only catalogue and then a note again should leave undo history where it belongs. */
  const editable = new Compartment()

  /** The window's colours, handed to CodeMirror as a theme of the right facet rather than left to a CSS race.
   *
   *  Asking CodeMirror for its dark facet is the part that matters. Untold, it paints its light defaults — caret
   *  `1.2px solid black`, bright blue selection, native caret `black` — while our stylesheet paints near-black
   *  paper, and a tester reads that as "the cursor is near invisible in dark mode". Through a theme the colours
   *  arrive inside CM's own style module, where they cannot be out-injected.
   *
   *  The palette stays in CSS (`--ink`, `--paper`): this hands over `var(…)`, so dark mode still means one set of
   *  numbers, in `app.css`, rather than a second copy that can drift. */
  function jottTheme(scheme: Scheme): Extension {
    const dark = scheme === 'dark'
    return [
      EditorView.darkTheme.of(dark),
      // CM's own dark/light markers are generated class names (`ͼ2_…`), which is fine for a stylesheet and useless
      // to a person opening devtools — or to a test. This says, in the DOM, which scheme the editor was told.
      EditorView.editorAttributes.of({ class: dark ? 'jott-dark' : 'jott-light' }),
      EditorView.theme(
        {
          '&': { backgroundColor: 'var(--paper)', color: 'var(--ink)' },
          // CM draws the caret itself and hides the native one; both answers come from here, so neither can be a
          // light-facet default on dark paper.
          '.cm-content': { caretColor: 'transparent' },
          '.cm-cursor, .cm-dropCursor': { borderLeft: '2px solid var(--ink)', marginLeft: '-1px' },
          '.cm-selectionBackground, .cm-content ::selection': {
            backgroundColor: dark ? '#3a4a5e' : '#cfe3f5',
          },
          '.cm-activeLine': { backgroundColor: dark ? '#242221' : '#f2f1ef' },
        },
        { dark },
      ),
    ]
  }

  /** Where the query's words are in this text, marked. Offsets come from `highlight.ts`, which is also what
   * the result list marks with — the row and the note it opens cannot disagree about what matched (§18.11). */
  function highlightExtension(query: string, prefix: boolean): Extension {
    if (!query) return []
    const mark = Decoration.mark({ class: 'cm-jott-match' })
    const build = (state: EditorState) =>
      RangeSet.of(
        matchRanges(state.doc.toString(), query, prefix).map((range) => mark.range(range.from, range.to)),
        true,
      )
    return StateField.define<ReturnType<typeof build>>({
      create: build,
      update: (value, transaction) => (transaction.docChanged ? build(transaction.state) : value),
      provide: (field) => EditorView.decorations.from(field),
    })
  }

  let host: HTMLDivElement | undefined = $state()
  let view: EditorView | undefined
  /** The text this buffer last matched the file with, and its hash — the reference every question below
   * is asked against (`dirty`, `diskChanged`, `saved`). */
  let baseline = ''
  let baselineHash = ''
  let currentHash = $state('')
  let savedHash: string | undefined = $state()
  let saving = $state(false)

  const phase: SavePhase = $derived(
    saveState({
      dirty: currentHash !== baselineHash,
      // Compared against the bytes we read or wrote, never against "some time ago" (§14).
      diskChanged: note !== null && note.diskHash !== baselineHash,
      saving,
      savedHash,
      currentHash,
    }),
  )

  $effect(() => onphase?.(phase))

  // The desktop's scheme, tracked rather than sampled once: Windows follows the clock, and a window that painted
  // light ink on dark paper after 19:00 would be the same bug wearing a different hour.
  $effect(() => {
    const stop = watchScheme((next) => (scheme = next))
    return () => stop()
  })

  /** A scheme change reconfigures the theme only. The buffer, the caret's history and anything unsaved stay where
   *  they are — rebuilding the editor to change a colour would be the cure that kills the patient (§18.13). */
  $effect(() => {
    const wanted = scheme
    const viewNow = view
    if (!viewNow) return
    untrack(() => viewNow.dispatch({ effects: themer.reconfigure(jottTheme(wanted)) }))
  })

  let mirrorTimer: ReturnType<typeof setTimeout> | undefined
  let saveTimer: ReturnType<typeof setTimeout> | undefined

  /** What scheme the desktop asked for, followed while this window is open. */
  let scheme = $state<Scheme>(currentScheme())

  function extensions() {
    return [
      themer.of(jottTheme(untrack(() => scheme))),
      // Note the negation, which is easy to get backwards: `readonly` describes the file, the facet describes
      // whether typing is accepted. Reading it straight hands out an editor for exactly the file nobody may edit.
      editable.of(EditorView.editable.of(!untrack(() => readonly))),
      history(),
      bracketMatching(),
      markdown(),
      // `fallback: true` tints headings and emphasis even before the parser has caught up with typing,
      // which is what the Windows hand-off is asked to look at (§9).
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      // Markdown that reads like its own rendering: headings sized, emphasis real, markers faded. Listed after
      // the default style so its rules win for the tokens markdown actually produces (§9's "readable first").
      syntaxHighlighting(markdownStyle, { fallback: true }),
      EditorView.lineWrapping,
      keymap.of([
        // Mod-s is CodeMirror's "jump to line" by default. Here it means save, and nothing else (§9).
        { key: 'Mod-s', mac: 'Mod-s', preventDefault: true, run: () => (void saveNow(), true) },
        ...defaultKeymap,
        ...historyKeymap,
        indentWithTab,
      ]),
      EditorView.domEventHandlers({ blur: () => (void saveNow(), true) }),
      // Read untracked on purpose: this runs inside the effect that opens a note, and a keystroke in the
      // search box must not re-run that effect — it would rebuild the buffer under someone typing (§4).
      highlighter.of(highlightExtension(untrack(() => highlight), untrack(() => highlightPrefix))),
    ]
  }

  /** The query changed while this note is open: swap the marks, and land the reader on the first one. Only
   * the highlighting is reconfigured — the text, the caret's history and any unsaved typing are untouched. */
  $effect(() => {
    const query = highlight
    // Tracked beside the query: flipping "match word beginnings" re-marks and re-scrolls this note, because the words
    // that matched genuinely changed and marks left over from the other mode would be a lie about the previous search.
    const prefix = highlightPrefix
    const viewNow = view
    if (!viewNow) return
    untrack(() => {
      viewNow.dispatch({ effects: highlighter.reconfigure(highlightExtension(query, prefix)) })
      if (!query) return
      const [first] = matchRanges(viewNow.state.doc.toString(), query, prefix)
      if (!first) return
      // Vector-only hits have no words to land on, and jumping to the top of a note nobody matched is worse
      // than not moving at all.
      viewNow.dispatch({ selection: { anchor: first.to }, effects: EditorView.scrollIntoView(first.from, { y: 'center' }) })
    })
  })

  function open(text: string) {
    view?.destroy()
    if (!host) return
    view = new EditorView({
      state: EditorState.create({ doc: text, extensions: [...extensions(), EditorView.updateListener.of(onUpdate)] }),
      parent: host,
    })
  }

  // Which note these numbers describe. Hashing is async and a fast switch (or a save that finishes after
  // one) would otherwise let the previous note's digest land as the new note's baseline — after which the
  // window calls untouched text "unsaved" and offers to write it somewhere it was never typed (§4).
  let generation = 0

  function setBaseline(text: string, hash: string) {
    baseline = text
    baselineHash = hash
    currentHash = hash
    // "Saved" is a claim about bytes this app wrote (§11). Reading a file makes no such claim.
    savedHash = undefined
  }

  async function saveNow() {
    // Nothing to write, and nothing that should ever be written: np's catalogue is regenerated by np. Autosave,
    // blur and Ctrl+S all funnel through here, so the guard belongs once, in the one path they share.
    if (readonly) return
    if (!rel || !view || saving) return
    const which = rel
    const gen = generation
    const text = view.state.doc.toString()
    const hash = await hashText(text)
    // The note changed while the digest was being computed: this text belongs to a buffer that is gone.
    if (which !== rel || gen !== generation) return
    if (!wantsSave({ dirty: hash !== baselineHash, diskChanged: (note?.diskHash ?? '') !== baselineHash, saving, savedHash, currentHash: hash })) {
      return
    }
    saving = true
    try {
      const result = await onsave(which, text)
      if (which !== rel || gen !== generation) return
      // The answer carries the hash of the bytes that landed, so `saved` is a fact about the disk (§11).
      setBaseline(text, result.hash)
      savedHash = result.hash
    } finally {
      saving = false
    }
  }

  // A file that arrived read-only, or a note opened after a catalogue: the same view, one facet changed. The text
  // stays selectable and copyable — refusing to edit was never a reason to withhold what it says.
  $effect(() => {
    const writable = !readonly
    const viewNow = view
    if (!viewNow) return
    untrack(() => viewNow.dispatch({ effects: editable.reconfigure(EditorView.editable.of(writable)) }))
  })

  /* Whether the caret is owed. CodeMirror draws its own and hides the browser's, and it shows its own only under
     `.cm-focused` — a class it sets from its own focus observers:

         ".cm-cursor": { display: "none" }
         "&.cm-focused > .cm-scroller > .cm-cursorLayer .cm-cursor": { display: "block" }

     Two releases went by colouring and widening a caret that was never drawn, because `display: none` outranks both.
     So the window records focus itself, from the DOM events that fire when focus genuinely arrives, and the stylesheet
     below draws on that. `relatedTarget` keeps an intra-editor move (clicking from one line to the next) from reading
     as leaving. */
  let caretFocused = $state(false)
  $effect(() => {
    const node = host
    if (!node) return
    const arrived = () => (caretFocused = true)
    const left = (event: FocusEvent) => {
      const goingTo = event.relatedTarget
      if (goingTo instanceof Node && node.contains(goingTo)) return
      caretFocused = false
    }
    node.addEventListener('focusin', arrived)
    node.addEventListener('focusout', left)
    return () => {
      node.removeEventListener('focusin', arrived)
      node.removeEventListener('focusout', left)
      caretFocused = false
    }
  })

  // Ctrl+S from the window. `> 0` so mounting an editor does not save a note nobody touched.
  $effect(() => {
    if (saveSignal > 0) void saveNow()
  })

  function schedule() {
    clearTimeout(mirrorTimer)
    clearTimeout(saveTimer)
    if (!rel) return
    mirrorTimer = setTimeout(() => {
      const text = view?.state.doc.toString()
      if (rel && text !== undefined && text !== baseline) onmirror(rel, text)
    }, MIRROR_AFTER_MS)
    saveTimer = setTimeout(() => void saveNow(), AUTOSAVE_AFTER_MS)
  }

  function onUpdate(update: ViewUpdate) {
    // Anything that is not a change to the text (a cursor move, a scroll) must not start a save timer.
    if (!update.docChanged || !view) return
    const text = view.state.doc.toString()
    void hashText(text).then((hash) => (currentHash = hash))
    schedule()
  }

  // A different note: fill the editor with it. The same note whose file changed is not touched — that is
  // §14's conversation, held in the banner below.
  $effect(() => {
    const which = rel
    if (!which || !host) return
    // `rel` is the only thing tracked. The text is read once, untracked: App replaces `note` after every
    // save and on every outside change, and rebuilding an editor from that signal would throw away the
    // buffer, the caret and the undo history the moment someone pressed Ctrl+S (§4). Content arriving from
    // outside has its own effect below, driven by a token the parent bumps deliberately.
    const text = untrack(() => note?.text ?? '')
    const gen = ++generation
    void hashText(text).then((hash) => {
      if (gen === generation) setBaseline(text, hash)
    })
    open(text)
    // The `rel` this buffer was opened as, captured rather than read later: the parent changes `rel` to switch notes,
    // and a teardown flush has to be about the file whose text is on screen.
    const openedRel = rel
    return () => {
      clearTimeout(mirrorTimer)
      clearTimeout(saveTimer)
      // A buffer being thrown away mid-tick must not lose typing (§18.13). The mirror tick is half a second after
      // the last keystroke, and this cleanup used to cancel it — so closing a note, pressing Escape, or switching
      // folder inside that window left unsaved text with nothing on record about it. Closing still writes nothing to
      // the note itself: `onmirror` is core's memory of an unsaved buffer, not a save.
      const leaving = view?.state.doc.toString()
      if (openedRel && leaving !== undefined && leaving !== baseline) onmirror(openedRel, leaving)
      view?.destroy()
      view = undefined
    }
  })

  // Text arriving from outside the editor: a reload after a conflict, or an accepted mirror.
  $effect(() => {
    const token = external?.token ?? 0
    if (!token || !view) return
    const text = external!.text
    const gen = ++generation
    void hashText(text).then((hash) => {
      if (gen === generation) setBaseline(text, hash)
    })
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } })
  })

  function onPageHide() {
    // Unload is a save point (§9). Whether the IPC call survives the webview going away is not something
    // this app gets to assume — which is why §18.13 mirrors the buffer as well.
    void saveNow()
  }
  window.addEventListener('pagehide', onPageHide)
  onDestroy(() => {
    window.removeEventListener('pagehide', onPageHide)
    clearTimeout(mirrorTimer)
    clearTimeout(saveTimer)
    view?.destroy()
    view = undefined
  })

  /** The banner's character count is the only place this file counts anything; it is a hint, not a claim. */
  const mirroredLength = $derived(note?.mirrored ? note.mirrored.text.length : 0)
</script>

<section class="editor">
  {#if readonly && rel}
    <p class="banner quiet" data-testid="read-only">
      This is note-processor’s own index. Read it if you like — Jott will not edit it, and np rewrites it on the next pass.
    </p>
  {/if}
  {#if !rel}
    <p class="placeholder" data-testid="no-note">Choose a note on the left, or make one.</p>
  {:else}
    {#if phase === 'conflict' && !kept}
      <div class="banner" role="alert">
        <span>This file changed outside Jott while you had unsaved typing. Nothing has been overwritten.</span>
        <button type="button" onclick={onreload}>Reload the file</button>
        <button type="button" onclick={onkeep}>Keep typing</button>
      </div>
    {/if}

    {#if note?.mirrored}
      <div class="banner">
        <span>You had unsaved text from an earlier session ({mirroredLength} characters).</span>
        <button type="button" onclick={onusemirror}>Use it</button>
        <button type="button" onclick={ondiscardmirror}>Discard it</button>
      </div>
    {/if}

    <div class="path" title={rel}>{rel}</div>
    <div class="cm" class:jott-caret-focus={caretFocused} bind:this={host}></div>
    <div class="phase" data-phase={phase}>
      {#if phase === 'saving'}saving{:else if phase === 'conflict'}unsaved · file changed outside{:else if phase === 'dirty'}unsaved{:else if phase === 'saved'}saved{:else}up to date{/if}
    </div>
  {/if}
</section>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    height: 100%;
  }
  .placeholder {
    margin: auto;
    color: color-mix(in srgb, currentColor 50%, transparent);
  }
  .path {
    padding: 0.25rem 0.75rem;
    font-size: 0.75rem;
    color: color-mix(in srgb, currentColor 60%, transparent);
    border-bottom: 1px solid color-mix(in srgb, currentColor 12%, transparent);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cm {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .cm :global(.cm-editor) {
    height: 100%;
    /* CodeMirror's default surface is white with black text, which is a bright rectangle in a dark window
       and fights the ink everything else mixes from `currentColor`. Take the palette; caret and selection
       follow `color` from there. */
    background: var(--paper);
    color: var(--ink);
  }
  /* Two answers to "is there a caret", in the order the browser reads them.

     First: CodeMirror's own drawn caret, shown whenever the DOM says focus is in this editor — not merely when
     CodeMirror's `.cm-focused` agrees. `min-height` is the guard for a line measured at zero height (a font still
     loading, a webview reporting an empty client rect): the height arrives as an inline style, so without it the caret
     is zero pixels tall and the window has no way to say so.

     Second: if CodeMirror does *not* believe it is focused, its layer is `display: none` and nothing above will show.
     Then the browser's real caret takes over — which needs an `!important`, because CodeMirror hides it with
     `caretColor: "transparent !important"` on `.cm-content`. Scoping that to `:not(.cm-focused)` is what keeps exactly
     one caret on screen instead of two blinking at each other. */
  .cm.jott-caret-focus :global(.cm-editor .cm-cursor),
  .cm.jott-caret-focus :global(.cm-editor .cm-dropCursor) {
    display: block;
    min-height: 1em;
  }
  .cm.jott-caret-focus :global(.cm-editor:not(.cm-focused) .cm-content) {
    caret-color: var(--ink) !important;
  }

  .cm :global(.cm-editor .cm-cursor),
  .cm :global(.cm-editor.cm-focused .cm-cursor),
  .cm :global(.cm-editor .cm-dropCursor) {
    /* The same ruler as the theme above, kept as a second answer: if a build ever ships without the theme
       extension, the caret is still two pixels of ink rather than CodeMirror's 1.2px black. */
    border-left: 2px solid var(--ink);
  }

  /* CodeMirror blinks with `steps(1)` down to `opacity: 0`, which means the caret is genuinely absent for half of
     every 1.2 s cycle. That is what "I cannot see where the cursor is" describes even when the colour is right, so
     here it dims instead of switching off — and stands still for anyone whose OS already said no motion. */
  .cm :global(.cm-editor.cm-focused > .cm-scroller > .cm-cursorLayer) {
    animation-name: jott-caret;
  }

  @keyframes jott-caret {
    0%,
    62% {
      opacity: 1;
    }
    63%,
    100% {
      opacity: 0.4;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .cm :global(.cm-editor.cm-focused > .cm-scroller > .cm-cursorLayer) {
      animation: none;
    }
  }

  /* Windows high-contrast (forced colors) replaces custom properties, so `var(--ink)` is exactly what does not
     survive there. The system's text-on-canvas colour does. */
  @media (forced-colors: active) {
    .cm :global(.cm-editor .cm-cursor),
    .cm :global(.cm-editor .cm-dropCursor) {
      border-left-color: CanvasText !important;
    }
  }
  /* ---- markdown, styled. The classes come from `markdownStyle.ts`; a class named there and not here renders
        as plain text, which a test refuses. ------------------------------------------------------------- */
  .cm :global(.md-h1) {
    font-size: 1.5em;
    font-weight: 700;
    line-height: 1.25;
  }
  .cm :global(.md-h2) {
    font-size: 1.3em;
    font-weight: 700;
    line-height: 1.3;
  }
  .cm :global(.md-h3) {
    font-size: 1.15em;
    font-weight: 600;
  }
  .cm :global(.md-h4) {
    font-weight: 600;
  }
  .cm :global(.md-h5) {
    font-weight: 600;
    font-style: italic;
  }
  .cm :global(.md-h6) {
    font-weight: 600;
    /* A level-six heading in a note is usually a label, not a title. */
    color: color-mix(in srgb, currentColor 72%, transparent);
  }
  .cm :global(.md-strong) {
    font-weight: 700;
  }
  .cm :global(.md-em) {
    font-style: italic;
  }
  .cm :global(.md-strike) {
    text-decoration: line-through;
  }
  .cm :global(.md-code) {
    background: color-mix(in srgb, currentColor 10%, transparent);
    border-radius: 3px;
  }
  .cm :global(.md-link) {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .cm :global(.md-quote) {
    color: color-mix(in srgb, currentColor 78%, transparent);
    font-style: italic;
  }
  .cm :global(.md-list) {
    /* The bullet or number itself. Ink enough to see, quiet enough not to shout over the item. */
    color: color-mix(in srgb, currentColor 65%, transparent);
  }
  .cm :global(.md-meta) {
    /* `#`, `**`, `` ` ``, `>`: present, faded. See `markdownStyle.ts` for why not hidden. */
    color: color-mix(in srgb, currentColor 42%, transparent);
  }
  .cm :global(.cm-jott-match) {
    /* A tint of the ink rather than a fixed yellow, so it reads on both papers (see the palette tests). */
    background: color-mix(in srgb, currentColor 24%, transparent);
    border-radius: 2px;
  }
  .cm :global(.cm-gutters) {
    background: var(--paper);
    color: color-mix(in srgb, currentColor 55%, transparent);
  }
  .cm :global(.cm-activeLine),
  .cm :global(.cm-activeLineGutter) {
    /* A tint of the ink, not a fixed grey, so the current line reads on both papers. */
    background: color-mix(in srgb, currentColor 6%, transparent);
  }
  .cm :global(.cm-scroller) {
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
    line-height: 1.5;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.75rem;
    background: color-mix(in srgb, #b8863b 16%, transparent);
    font-size: 0.8125rem;
  }
  .banner button {
    border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
    background: none;
    color: inherit;
    font: inherit;
    padding: 0.05rem 0.45rem;
    cursor: default;
  }
  .phase {
    padding: 0.15rem 0.75rem;
    font-size: 0.75rem;
    color: color-mix(in srgb, currentColor 55%, transparent);
    border-top: 1px solid color-mix(in srgb, currentColor 12%, transparent);
  }
  .phase[data-phase='dirty'],
  .phase[data-phase='conflict'] {
    color: #b8863b;
  }
  :global(.dark) .phase[data-phase='dirty'],
  :global(.dark) .phase[data-phase='conflict'] {
    color: #d9a441;
  }
</style>
