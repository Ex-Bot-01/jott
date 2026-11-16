//! `getCurrentWindow`, aliased over `@tauri-apps/api/window` for the harness. Only focus is asked about (§4's "notify
//! only when the window is not looking"), and a browser tab has an honest answer to that.

export function getCurrentWindow() {
  return {
    isFocused: async () => (typeof document === 'undefined' ? true : !document.hidden),
    show: async () => undefined,
    setFocus: async () => undefined,
  }
}
