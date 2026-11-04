/** How long the arrow keys must be quiet before the jump is sent (ms). */
export const QUIET_MS = 250;
/** The longest a held key waits before a jump is sent anyway, so the picture still follows it (ms). */
export const MAX_WAIT_MS = 1000;

/** The clock and timers the scheduler uses; tests bring their own. */
export interface Timers {
  now: () => number;
  setTimer: (action: () => void, ms: number) => unknown;
  clearTimer: (handle: unknown) => void;
}

const realTimers: Timers = {
  now: () => Date.now(),
  setTimer: (action, ms) => setTimeout(action, ms),
  clearTimer: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

export interface SeekScheduler {
  /** Asks for a jump to `targetMs`; a newer ask replaces it before it is sent. */
  press: (targetMs: number) => void;
  /** Sends a jump at once and forgets any waiting one (a click on the seek bar). */
  sendNow: (targetMs: number) => void;
  /** Forgets a waiting jump without sending it (the player closed). */
  cancel: () => void;
}

/**
 * Turns a burst of key presses into a few jumps. Each jump makes the player fetch the video again
 * from the new place, and a server that limits requests answers a flood of them with "429 Too Many
 * Requests", after which the player skips minutes of video. So a jump is sent once the keys have
 * been quiet for [`QUIET_MS`], and at most every [`MAX_WAIT_MS`] while a key is held down.
 */
export function createSeekScheduler(
  send: (targetMs: number) => void,
  timers: Timers = realTimers,
): SeekScheduler {
  let waiting: unknown = null;
  let target = 0;
  let firstPressAt: number | null = null;

  const clear = () => {
    if (waiting !== null) timers.clearTimer(waiting);
    waiting = null;
    firstPressAt = null;
  };

  const flush = () => {
    const place = target;
    clear();
    send(place);
  };

  return {
    press: (targetMs) => {
      target = targetMs;
      const now = timers.now();
      firstPressAt ??= now;
      if (now - firstPressAt >= MAX_WAIT_MS) {
        flush();
        return;
      }
      if (waiting !== null) timers.clearTimer(waiting);
      waiting = timers.setTimer(flush, QUIET_MS);
    },
    sendNow: (targetMs) => {
      clear();
      send(targetMs);
    },
    cancel: clear,
  };
}
