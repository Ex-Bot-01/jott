//! Notifications, aliased over `@tauri-apps/plugin-notification` for the harness. Nothing leaves the page: the body is
//! printed and pushed onto `window.__harnessNotifications`, so "did we notify, and with what words" stays checkable.

export async function isPermissionGranted(): Promise<boolean> {
  return true
}

export async function requestPermission(): Promise<'granted' | 'denied'> {
  return typeof location !== 'undefined' && new URLSearchParams(location.search).get('notify') === 'deny' ? 'denied' : 'granted'
}

export function sendNotification(options: { title?: string; body?: string }): void {
  const sink = ((globalThis as Record<string, unknown>).__harnessNotifications as unknown[] | undefined) ?? []
  ;(globalThis as Record<string, unknown>).__harnessNotifications = sink
  sink.push({ ...options, at: Date.now() })
  console.info('[harness notification]', options.title, options.body)
}
