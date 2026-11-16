//! `listen`, aliased over `@tauri-apps/api/event` for `npm run ui`. Events come from `bus` in `harnessState.ts`, so a
//! console call — `__harness_emit('ingest://event', {...})` — drives the status line the way np's progress lines do.

import { bus } from './harnessState'

export async function listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> {
  const stop = bus.listen(event, (payload) => handler({ payload } as { payload: T }))
  return () => {
    stop()
  }
}
