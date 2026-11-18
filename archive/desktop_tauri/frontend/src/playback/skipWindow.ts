import { SkipPrompt } from './skipPrompt';

/** How long a skip button stays on screen on its own when its part of the video begins (ms). */
export const WINDOW_MS = 5000;

/** The clock the window uses; tests bring their own. */
export interface WindowTimers {
  setTimer: (action: () => void, ms: number) => unknown;
  clearTimer: (handle: unknown) => void;
}

const realTimers: WindowTimers = {
  setTimer: (action, ms) => setTimeout(action, ms),
  clearTimer: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/**
 * Names the prompt, so a new part of the video (or leaving one and entering it again) is told apart
 * from the same part going on. Null when there is no prompt.
 */
export function promptKey(prompt: SkipPrompt | null): string | null {
  if (!prompt) return null;
  return prompt.action === 'next' ? 'next' : `seek:${prompt.toMs}`;
}

/**
 * Whether the button is on screen: only while there is something to offer, and then for the first
 * [`WINDOW_MS`] of its part of the video, or whenever the player's controls are showing.
 */
export function skipButtonVisible(input: {
  hasPrompt: boolean;
  windowOpen: boolean;
  controlsVisible: boolean;
}): boolean {
  return input.hasPrompt && (input.windowOpen || input.controlsVisible);
}

export interface PromptWindow {
  /** Tells the window which prompt is current (null for none); a different one opens a fresh window. */
  show: (key: string | null) => void;
  /** Stops the timer (the player closed). */
  stop: () => void;
}

/**
 * The first few seconds of a prompt: opens when a prompt appears, closes after [`WINDOW_MS`] or as
 * soon as the prompt goes away. The same prompt told again changes nothing, so the time is not
 * restarted by every position update.
 */
export function createPromptWindow(
  onChange: (open: boolean) => void,
  timers: WindowTimers = realTimers,
  windowMs: number = WINDOW_MS,
): PromptWindow {
  let current: string | null = null;
  let timer: unknown = null;

  const clear = () => {
    if (timer !== null) timers.clearTimer(timer);
    timer = null;
  };

  return {
    show: (key) => {
      if (key === current) return;
      clear();
      current = key;
      if (key === null) {
        onChange(false);
        return;
      }
      onChange(true);
      timer = timers.setTimer(() => {
        timer = null;
        onChange(false);
      }, windowMs);
    },
    stop: clear,
  };
}
