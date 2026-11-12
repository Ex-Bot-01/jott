<script lang="ts">
  // The note itself: CodeMirror 6 with markdown tinting, line wrap on, no minimap (§9), and every
  // save-or-not decision taken by `saveState()` rather than by anything in this file (§11, §14).

  import { onDestroy, untrack } from 'svelte'
  import { EditorView, keymap, type ViewUpdate } from '@codemirror/view'
  import { EditorState } from '@codemirror/state'
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
  import { markdown } from '@codemirror/lang-markdown'
  import { bracketMatching, syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
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
    kept = false,
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
    /** "Keep typing" was pressed for this note: the banner has been answered, so it stands down. */
    kept?: boolean
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

  let mirrorTimer: ReturnType<typeof setTimeout> | undefined
  let saveTimer: ReturnType<typeof setTimeout> | undefined

  function extensions() {
    return [
      history(),
      bracketMatching(),
      markdown(),
      // `fallback: true` tints headings and emphasis even before the parser has caught up with typing,
      // which is what the Windows hand-off is asked to look at (§9).
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      EditorView.lineWrapping,
      keymap.of([
        // Mod-s is CodeMirror's "jump to line" by default. Here it means save, and nothing else (§9).
        { key: 'Mod-s', mac: 'Mod-s', preventDefault: true, run: () => (void saveNow(), true) },
        ...defaultKeymap,
        ...historyKeymap,
        indentWithTab,
      ]),
      EditorView.domEventHandlers({ blur: () => (void saveNow(), true) }),
    ]
  }

  function open(text: string) {
    view?.destroy()
    if (!host) return
    view = new EditorView({
      state: EditorState.create({ doc: text, extensions: [...extensions(), EditorView.updateListener.of(onUpdate)] }),
      parent: host,
    })
  }

  function setBaseline(text: string, hash: string) {
    baseline = text
    baselineHash = hash
    currentHash = hash
    // "Saved" is a claim about bytes this app wrote (§11). Reading a file makes no such claim.
    savedHash = undefined
  }

  async function saveNow() {
    if (!rel || !view || saving) return
    const text = view.state.doc.toString()
    const hash = await hashText(text)
    if (!wantsSave({ dirty: hash !== baselineHash, diskChanged: (note?.diskHash ?? '') !== baselineHash, saving, savedHash, currentHash: hash })) {
      return
    }
    saving = true
    try {
      const result = await onsave(rel, text)
      // The answer carries the hash of the bytes that landed, so `saved` is a fact about the disk (§11).
      setBaseline(text, result.hash)
      savedHash = result.hash
    } finally {
      saving = false
    }
  }

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
    void hashText(text).then((hash) => setBaseline(text, hash))
    open(text)
    return () => {
      clearTimeout(mirrorTimer)
      clearTimeout(saveTimer)
      view?.destroy()
      view = undefined
    }
  })

  // Text arriving from outside the editor: a reload after a conflict, or an accepted mirror.
  $effect(() => {
    const token = external?.token ?? 0
    if (!token || !view) return
    const text = external!.text
    void hashText(text).then((hash) => setBaseline(text, hash))
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
  {#if !rel}
    <p class="placeholder">Choose a note on the left, or make one.</p>
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
    <div class="cm" bind:this={host}></div>
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
