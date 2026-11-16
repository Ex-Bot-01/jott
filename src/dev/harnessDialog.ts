//! The native dialogs, aliased over `@tauri-apps/plugin-dialog` for the harness. A real picker cannot run headless, so
//! the answer is scripted: `?pick=cancel` makes it cancel, anything else picks the fixture folder.

export async function open(options?: unknown): Promise<string | null> {
  void options
  if (typeof location !== 'undefined' && new URLSearchParams(location.search).get('pick') === 'cancel') return null
  return '/home/example/Notes'
}

/** `confirm` answers what the harness is told to answer, because "declined" is a state worth screenshotting too. */
export async function confirm(message: string, options?: unknown): Promise<boolean> {
  void message
  void options
  if (typeof location === 'undefined') return true
  return new URLSearchParams(location.search).get('confirm') !== 'no'
}
