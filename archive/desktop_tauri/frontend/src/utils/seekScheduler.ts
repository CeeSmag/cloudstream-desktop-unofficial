/** The shortest time between two jumps sent to the player while keys are tapped (ms). */
export const FAST_GAP_MS = 200;
/** The shortest time between two jumps once a key has been held or tapped for [`SUSTAINED_MS`] (ms). */
export const SLOW_GAP_MS = 500;
/** How long a run of presses lasts before it counts as a held key (ms). */
export const SUSTAINED_MS = 1000;
/** A pause between presses this long ends a run; the next press starts a new one (ms). */
export const RUN_ENDS_MS = 400;

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
  /** Asks for a jump to `targetMs`: sent at once, or as soon as the last jump is old enough. */
  press: (targetMs: number) => void;
  /** Sends a jump at once and forgets any waiting one (a click on the seek bar). */
  sendNow: (targetMs: number) => void;
  /** Forgets a waiting jump without sending it (the player closed). */
  cancel: () => void;
}

/**
 * Sends the first press of the arrow keys to the player at once and spaces out the ones that
 * follow. Each jump can make the player fetch the video again from the new place, and a server that
 * limits requests answers a flood of them with "429 Too Many Requests", after which the player
 * skips minutes of video. So after a jump the next one waits until [`FAST_GAP_MS`] have passed
 * (carrying the newest target), and a key held for longer than [`SUSTAINED_MS`] sends one only
 * every [`SLOW_GAP_MS`].
 */
export function createSeekScheduler(
  send: (targetMs: number) => void,
  timers: Timers = realTimers,
): SeekScheduler {
  let waiting: unknown = null;
  let target = 0;
  let lastSentAt: number | null = null;
  let lastPressAt: number | null = null;
  let runStartedAt = 0;

  const clear = () => {
    if (waiting !== null) timers.clearTimer(waiting);
    waiting = null;
  };

  const sendTarget = () => {
    clear();
    lastSentAt = timers.now();
    send(target);
  };

  return {
    press: (targetMs) => {
      target = targetMs;
      const now = timers.now();
      if (lastPressAt === null || now - lastPressAt >= RUN_ENDS_MS) runStartedAt = now;
      lastPressAt = now;
      const gap = now - runStartedAt >= SUSTAINED_MS ? SLOW_GAP_MS : FAST_GAP_MS;
      const wait = lastSentAt === null ? 0 : lastSentAt + gap - now;
      if (wait <= 0) {
        sendTarget();
        return;
      }
      clear();
      waiting = timers.setTimer(sendTarget, wait);
    },
    sendNow: (targetMs) => {
      target = targetMs;
      sendTarget();
    },
    cancel: clear,
  };
}
