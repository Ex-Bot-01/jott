import { describe, expect, it } from 'vitest'
import { formatsSentence, opensAsText, planNoteName } from './noteName'

const named = (name: string, folder = '') => planNoteName(name, folder)

describe('the name a new note actually gets', () => {
  it('adds .md when the name has no extension at all', () => {
    // "I just entered the name of the file and it just created this — no markdown file." A note that is not `.md` is
    // not a note to note-processor, so the intention was lost at the last step.
    expect(named('hello')).toEqual({ rel: 'hello.md', said: null })
    expect(named('trip plan')).toEqual({ rel: 'trip plan.md', said: null })
  })

  it('keeps a markdown name as it is, and folds the one spelling note-processor cannot read', () => {
    expect(named('hello.md')).toEqual({ rel: 'hello.md', said: null })
    // Measured against np 0.5.1: a file is a note when its lowercased path ends in `.md`. `.markdown` is not that, so
    // leaving it alone would create something nobody searches — and renaming it silently would be worse than saying it.
    const folded = named('hello.markdown')
    expect(folded.rel).toBe('hello.md')
    expect(folded.said).toContain('hello.md')
    expect(folded.said).toContain('.markdown')
  })

  it('leaves a name whose extension means something, and says what note-processor will do with it', () => {
    for (const typed of ['hello.txt', 'hello.png', 'hello.JPG', 'notes.json']) {
      const result = named(typed)
      expect(result.rel).toBe(typed)
      expect((result.said ?? '').toLowerCase()).toContain('.md')
    }
  })

  it('treats a digit after the last dot as part of the name, not an extension', () => {
    // `v1.2` is a name someone typed, not a file type: appending nothing would ask for a file called `v1.2`.
    expect(named('v1.2')).toEqual({ rel: 'v1.2.md', said: null })
  })

  it('keeps a long dotted name and admits what it is', () => {
    // Predictable beats clever: the last segment decides, so `hello.tar.gz` stays exactly what was typed rather than
    // being quietly rewritten to something nobody asked for.
    const result = named('hello.tar.gz')
    expect(result.rel).toBe('hello.tar.gz')
    expect((result.said ?? '').toLowerCase()).toContain('.md')
  })

  it('puts the note inside the folder that was chosen', () => {
    expect(named('hello', 'notes/')).toEqual({ rel: 'notes/hello.md', said: null })
  })

  it('refuses names that cannot become a file, in words, rather than renaming them', () => {
    for (const typed of ['', '   ', '.', '..', 'hello.']) {
      const result = named(typed)
      expect(result.rel).toBeNull()
      expect((result.said ?? '').length, `"${typed}" gets a sentence, not a shrug`).toBeGreaterThan(0)
    }
  })

  it('refuses a name Windows resolves to a device, and says which rule it hit', () => {
    // Issue #5. `nul`, `AUX.md`, `com1.md`: Windows looks the *stem* up in its legacy device namespace before it
    // looks at the folder, so `save_note("nul.md")` wrote to NUL, reported "Saved", and left nothing in the vault —
    // a silent-success path on the platform Jott ships first. Core refuses these too (`check_rel`'s neighbour in
    // `files.rs`); this is the sentence for the person typing, so the two have to agree.
    for (const typed of ['nul', 'NUL', 'nul.md', 'AUX.md', 'com1.md', 'LPT9.txt', 'prn', 'notes/nul/plan']) {
      const result = named(typed)
      expect(result.rel, `"${typed}" is not a note`).toBeNull()
      expect((result.said ?? '').toLowerCase(), `"${typed}": ${result.said}`).toContain('windows')
    }
  })

  it('does not refuse an ordinary name that merely contains one of those stems', () => {
    // `null.md` and `auxiliary.md` are notes somebody has already written. The rule is about the stem before the
    // first dot, which is what Windows matches — nothing wider.
    for (const typed of ['null', 'auxiliary', 'comet notes', 'lpt', 'a.nul', 'nul-']) {
      const result = named(typed)
      expect(result.rel, `"${typed}" is a note`).not.toBeNull()
    }
  })

  it('warns that a dotted name will never appear in the tree either', () => {
    // Both walks hide names beginning with a dot — Jott's drops any segment starting with one, np excludes them at
    // every depth. The create succeeds, so the sentence has to say the file will be invisible here as well as
    // unsearchable, rather than "Created .keep." about something nobody will ever see again.
    const result = named('.keep')
    expect(result.rel).toBe('.keep')
    expect((result.said ?? '').toLowerCase()).toContain('dot')
  })

  it('refuses `.md` on its own', () => {
    // note-processor would read a file called `.md` as a note (its rule is a folded `ends_with(".md")`), but Jott's
    // walk gives a leading-dot name no extension and hides the segment — so it would be an invisible note.
    expect(named('.md').rel).toBeNull()
  })

  it('refuses a name that climbs out of the folder', () => {
    // Core refuses `../plan` and the window now draws that sentence, but the sibling planner for moves says the same
    // thing locally in friendlier words. Two ways to create should not mean two qualities of message.
    const escaped = named('../plan')
    expect(escaped.rel).toBeNull()
    expect((escaped.said ?? '').toLowerCase()).toContain('inside')
  })

  it('still allows a nested path typed from the dialog', () => {
    // The dialog's own label says "in a path from the vault root", so `work/2026/plan` is an intention, not an escape.
    expect(named('work/2026/plan')).toEqual({ rel: 'work/2026/plan.md', said: null })
  })

  it('does not refuse a name that merely begins with a dot', () => {
    // `.gitignore` is a real file someone means. It is not markdown, so the sentence about what np will do still comes.
    const result = named('.keep')
    expect(result.rel).toBe('.keep')
    expect((result.said ?? '').toLowerCase()).toContain('.md')
  })
})

describe('saying which files become notes', () => {
  it('names the rule and counts both sides of it', () => {
    const sentence = formatsSentence({ notes: 12, others: 3 })
    expect(sentence).toContain('12')
    expect(sentence).toContain('3')
    expect(sentence).toContain('.md')
  })

  it('is grammatical in the one case the sentence exists for', () => {
    // A folder of `.txt` files plus a single `.md` note is the reported shape, and the plural-only wording drew
    // "1 file is notes note-processor can search" — a sentence that reads as a bug in the product someone is
    // trusting to explain their data. Pinned exactly, because a loose `toContain('2')` also misses whether folder
    // rows and note-processor's own catalogue were counted as files.
    expect(formatsSentence({ notes: 1, others: 2 })).toBe(
      'Of 3 files here, 1 file is a note note-processor can search; 2 are kept as attachments — tracked for links, never searched. note-processor reads .md files as notes.',
    )
    expect(formatsSentence({ notes: 4, others: 1 })).toContain('4 files are notes note-processor can search')
  })

  it('says nothing when there is nothing to explain', () => {
    expect(formatsSentence({ notes: 4, others: 0 })).toBeNull()
  })

  it('does not call a folder of attachments an error', () => {
    // A vault of .txt and .docx files is someone's life, not a failure. The sentence has to be about what note-processor
    // reads, not about the person having done something wrong.
    const sentence = formatsSentence({ notes: 0, others: 40 }) ?? ''
    expect(sentence.toLowerCase()).not.toContain('error')
    expect(sentence.toLowerCase()).not.toContain('failed')
    expect(sentence).toContain('.md')
  })
})


describe('what the window does with what it made', () => {
  it('opens a note and a plain-text file in the editor', () => {
    expect(opensAsText('trip plan.md')).toBe(true)
    expect(opensAsText('todo.txt')).toBe(true)
    expect(opensAsText('notes.json')).toBe(true)
  })

  it('does not hand an editable buffer to a file its own tree calls an image', () => {
    // `+ note` keeps an extension typed on purpose, then used to open whatever came back. An empty `photo.png` opened
    // as prose-in-waiting: Ctrl+S would write sentences into a file Jott's walk labels Image and never parses.
    for (const name of ['photo.png', 'diagram.JPG', 'scan.pdf', 'clip.mp4', 'archive.zip']) {
      expect(opensAsText(name), name).toBe(false)
    }
  })

  it('opens a dotted name it warned about, because that is what was asked for', () => {
    // `.keep` is text; the sentence above already says the tree will not show it.
    expect(opensAsText('.keep')).toBe(true)
  })
})
