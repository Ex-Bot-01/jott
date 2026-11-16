/**
 * Whether there is somewhere to put a new note or folder, and what to say when there is not.
 *
 * Reported from Windows on a first run: click **+ folder**, type a name, press Enter — nothing. No message, no folder.
 *
 * What actually happened, checked against `v0.0.16` rather than remembered: the refusal *was* drawn — in the sidebar,
 * below the tree — and the dialog closed on the way there whether or not the command worked. So the person pressed
 * Enter, the thing they were looking at vanished, and a small grey sentence sat somewhere else. "Nothing happened" was
 * an accurate description of where their eyes were.
 *
 * This module exists so the answer arrives before a command is sent, in the dialog that stays open, beside the button
 * that fixes it — rather than as a report filed somewhere else after the fact.
 */

/** The words the dialog shows. It names what is missing *and* the way out, because "no" without a next step is what
 *  made the first five minutes of Jott feel broken. */
export const NO_FOLDER_OPEN = 'No folder is open yet, so there is nowhere to make this. Choose one, and it will be made inside it.'

/** `null` means go ahead. Anything else is the sentence to show instead of sending a command that would be refused. */
export function createBlocked(folderOpen: boolean): string | null {
  return folderOpen ? null : NO_FOLDER_OPEN
}
