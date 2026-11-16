//! The opener plugin, aliased for the harness. `?open=fail` makes every call reject, which is how the manual-copy
//! fallback in About and the "the desktop refused" line in the drawer get looked at without a desktop that refuses.

function shouldFail(): boolean {
  return typeof location !== 'undefined' && new URLSearchParams(location.search).get('open') === 'fail'
}

export async function openUrl(url: string): Promise<void> {
  if (shouldFail()) throw new Error(`the harness was told to refuse ${url}`)
  console.info('[harness opener] would open', url)
}

export async function revealItemInDir(path: string): Promise<void> {
  if (shouldFail()) throw new Error(`the harness was told to refuse ${path}`)
  console.info('[harness opener] would reveal', path)
}

export async function openPath(path: string): Promise<void> {
  return openUrl(`file://${path}`)
}
