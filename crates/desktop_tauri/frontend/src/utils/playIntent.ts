import { PlaybackStatus } from '../types';

/**
 * How long the viewer's last pause or play decides what the screen shows (ms). The player's state
 * is read a few times a second, so an answer that was already on its way can still say the
 * opposite; without this the button would flip back for a moment.
 */
export const INTENT_HOLD_MS = 1000;

/** What the viewer last asked for and when. */
export interface PlayIntent {
  paused: boolean;
  at: number;
}

function holds(intent: PlayIntent | null, now: number): intent is PlayIntent {
  return intent !== null && now - intent.at >= 0 && now - intent.at < INTENT_HOLD_MS;
}

/**
 * The status to show: the player's own, except that a fresh pause or play by the viewer wins over
 * a "Playing"/"Paused" that says the opposite. Loading, ended and failed states always show.
 */
export function withPlayIntent(status: PlaybackStatus, intent: PlayIntent | null, now: number): PlaybackStatus {
  if (!holds(intent, now)) return status;
  if (status.state !== 'Playing' && status.state !== 'Paused') return status;
  const state = intent.paused ? 'Paused' : 'Playing';
  return status.state === state ? status : { ...status, state };
}

/** Whether two statuses show the same thing, so the screen need not be drawn again. */
export function sameStatus(a: PlaybackStatus, b: PlaybackStatus): boolean {
  return a === b || JSON.stringify(a) === JSON.stringify(b);
}
