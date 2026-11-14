// Whether to send an OS notification, decided in one tested place rather than inside a callback chain.
//
// The window already says "model ready" in its own banner. An OS line is for the case where that banner is on a
// window nobody is looking at — which is why focus is half the test and not an afterthought (§4: say it once, in the
// place the person is actually looking).

export type Permission = 'granted' | 'denied' | 'not-determined'

/** Only when nobody is looking *and* the OS has said yes. `not-determined` is not a yes: asking is the caller's job,
    and this function reads the answer it came back with. */
export function shouldNotify(focused: boolean, permission: Permission): boolean {
  return !focused && permission === 'granted'
}

/** The taskbar line. Deliberately short of a size: np's byte count belongs in the window, where there is something
    to read it against, and a download that already finished has no use for a number nobody watched. */
export function notifyBody(modelId: string): string {
  return `Jott: ${modelId} is ready — search now matches meanings too`
}
