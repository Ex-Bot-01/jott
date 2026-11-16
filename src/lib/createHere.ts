/**
 * Whether there is somewhere to put a new note or folder, and what to say when there is not.
 *
 * Reported from Windows on a first run: click **+ folder**, type a name, press Enter — nothing. No message, no folder.
 * The window did produce a sentence ("no vault is open"), but it drew that sentence in a branch of the layout which
 * only renders when a folder is already open, so the refusal existed and nobody could read it. This module exists so
 * the check happens before a command is sent rather than being reported afterwards into the void.
 */

/** The words the dialog shows. It names what is missing *and* the way out, because "no" without a next step is what
 *  made the first five minutes of Jott feel broken. */
export const NO_FOLDER_OPEN = 'No folder is open yet, so there is nowhere to make this. Choose one, and it will be made inside it.'

/** `null` means go ahead. Anything else is the sentence to show instead of sending a command that would be refused. */
export function createBlocked(folderOpen: boolean): string | null {
  return folderOpen ? null : NO_FOLDER_OPEN
}
