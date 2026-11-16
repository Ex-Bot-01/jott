/**
 * The name a new note actually gets, and what to say when the result is not something note-processor reads as a note.
 *
 * Two facts meet here. Someone typing `hello` into *new note* means a note, and note-processor indexes exactly one kind
 * of file as one: measured against 0.5.1, and stated in its own walker, a file is a note when the lowercased relative
 * path ends in `.md`. Everything else it discovers is an attachment — hashed, tracked for links, never searched (np
 * spec §10). A folder of `notes.txt`, `ideas.org` and `scan.pdf` is therefore not an error and not nothing: it is a
 * folder whose contents Jott can list but cannot search.
 *
 * So the window adds `.md` when the name has no extension, leaves a name whose extension means something exactly as
 * typed, and says out loud what note-processor will do with the second kind. It never silently rewrites a name that
 * already carries an extension: guessing at `hello.tar.gz` would create a file nobody asked for.
 */

/** The only suffix note-processor parses as a note (np's own `MARKDOWN_SUFFIX`, matched case-insensitively). */
const MARKDOWN = 'md'
/** Spellings of markdown someone might type, which np would not read. Folded to `.md`, and the fold is announced. */
const MARKDOWN_SPELLINGS = ['markdown', 'mkd', 'mdown']
/** An extension-shaped suffix: a letter first, then letters and digits, like every file type anyone has named. */
const EXTENSION_SHAPE = /^[A-Za-z][A-Za-z0-9]{0,7}$/

export interface Named {
  /** The path to create, `null` when the name cannot become a file. */
  rel: string | null
  /** What the window says about the result — the fold it made, or what note-processor will not do. `null` is silence,
   *  which is right for `hello` → `hello.md`: that is the intention, not a surprise. */
  said: string | null
}

/** Decide the name from the last dot onwards. Everything before it belongs to the name itself. */
function suffixOf(name: string): string | null {
  const dot = name.lastIndexOf('.')
  if (dot === 0) {
    // A leading dot is the whole name in `.gitignore` and `.keep`, and the person who typed it means that file rather
    // than asking for `.keep.md`. So the leading dot counts as the separator here, and nothing else changes.
    return EXTENSION_SHAPE.test(name.slice(1)) ? name.slice(1).toLowerCase() : null
  }
  if (dot < 0) return null
  const suffix = name.slice(dot + 1)
  return EXTENSION_SHAPE.test(suffix) ? suffix.toLowerCase() : null
}

export function planNoteName(typed: string, folder = ''): Named {
  const name = typed.trim()
  // `.` and `..` are the two names a filesystem already owes you, and a bare dot is not a name at all. The check sits
  // before the extension rule so `.hidden` — a file someone means — stays allowed below.
  if (name === '' || name === '.' || name === '..') {
    return { rel: null, said: 'That is not a file name. Type a name, like `trip plan`.' }
  }
  if (name.endsWith('.')) {
    // A trailing full stop is refused rather than trimmed: on Windows it is not even kept by the filesystem, so
    // silently dropping it would create something with a different name from the one that was typed.
    return { rel: null, said: 'A file name cannot end in a full stop.' }
  }

  const target = `${folder}${name}`
  const suffix = suffixOf(name)

  if (suffix === null) return { rel: `${target}.${MARKDOWN}`, said: null }
  if (suffix === MARKDOWN) return { rel: target, said: null }
  if (MARKDOWN_SPELLINGS.includes(suffix)) {
    const folded = target.slice(0, target.lastIndexOf('.')) + `.${MARKDOWN}`
    // Said because it is a rename: np cannot read the spelling that was typed, so the alternative is creating a file
    // that would sit unindexed forever.
    return { rel: folded, said: `Saved as ${folded} — note-processor reads .md files, not .${suffix}.` }
  }

  // Kept exactly as typed. Someone who writes `todo.txt` gets `todo.txt`; the window's job is to say what happens to it
  // next, not to refuse a format because this program cannot edit it nicely.
  return { rel: target, said: `Created ${target}. note-processor reads only .md files as notes, so this one will not be searchable.` }
}

/**
 * The line that answers "I opened my notes folder and search finds none of them".
 *
 * Counts come from Jott's own tree walk — it already classifies every row as a note, an image or another attachment —
 * so no new claim is being made about note-processor beyond the `.md` rule above. A folder with nothing but notes says
 * nothing: there is nothing to explain.
 */
export function formatsSentence(counts: { notes: number; others: number }): string | null {
  if (counts.others === 0) return null
  const files = counts.notes + counts.others
  const read = `${counts.notes} ${counts.notes === 1 ? 'file is' : 'files are'} notes note-processor can search`
  const rest = `${counts.others} ${counts.others === 1 ? 'is' : 'are'} kept as attachments — tracked for links, never searched`
  return `Of ${files} files here, ${read}; ${rest}. note-processor reads .md files as notes.`
}
