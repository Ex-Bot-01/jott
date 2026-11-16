import { describe, expect, it } from 'vitest'
import { formatsSentence, planNoteName } from './noteName'

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
