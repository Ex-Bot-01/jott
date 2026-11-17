import { EditorState } from '@codemirror/state'
import { markdown } from '@codemirror/lang-markdown'
import { describe, expect, it } from 'vitest'
import { activeMarks } from './markdownState'

const at = (doc: string, caret: number) =>
  activeMarks(EditorState.create({ doc, selection: { anchor: caret }, extensions: [markdown()] }))

describe('active marks', () => {
  it('knows bold from italic from code from a link', () => {
    const doc = '**b** _i_ `c` [l](u)'
    expect(at(doc, 3)).toContain('bold')
    expect(at(doc, 7)).toContain('italic')
    expect(at(doc, 11)).toContain('code')
    expect(at(doc, 16)).toContain('link')
    // The very end of the text, immediately after the closing parenthesis. Touching a mark is not being inside it:
    // lighting Link here would tell the next reader that the caret is in a link that it has already left. (The plan
    // asked for offset 21, which is past the end of this 20-character document — `EditorState` throws rather than
    // clamping, so no editor can ever hold that caret.)
    expect(at(doc, 20)).toEqual([])
  })

  it('reports a nested mark as both', () => {
    expect(at('**a _b_ c**', 5)).toEqual(expect.arrayContaining(['bold', 'italic']))
  })

  it('reads the caret’s line for list, task and heading', () => {
    const doc = '- plain\n- [x] done\n### head'
    expect(at(doc, 2)).toEqual(['bullet'])
    expect(at(doc, 10)).toEqual(expect.arrayContaining(['bullet', 'task']))
    expect(at(doc, 20)).toEqual(['heading'])
  })

  it('does not mistake a minus in a sentence for a bullet', () => {
    expect(at('wind - north', 6)).toEqual([])
  })

  it('holds nothing in an empty note', () => {
    expect(at('', 0)).toEqual([])
  })
})
