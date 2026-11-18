/**
 * How long a jump counts as "still on its way" (ms). The player's position is read a few times a
 * second, so right after a jump it still shows the old place; a second key press in this time must
 * continue from the first jump's target, not from the old position.
 */
export const SETTLE_MS = 1500;

/** The place the last jump aimed at and when it was made. */
export interface PendingSeek {
  targetMs: number;
  at: number;
}

/**
 * Where a jump of `deltaMs` lands. It starts from the target of a jump made less than
 * {@link SETTLE_MS} ago, otherwise from the position the player reports, and stays within the video.
 */
export function seekTarget(input: {
  positionMs: number;
  durationMs: number;
  deltaMs: number;
  pending: PendingSeek | null;
  now: number;
}): number {
  const { positionMs, durationMs, deltaMs, pending, now } = input;
  const fresh = pending !== null && now - pending.at >= 0 && now - pending.at < SETTLE_MS;
  const start = fresh ? pending.targetMs : positionMs;
  const target = Math.max(0, start + deltaMs);
  return durationMs > 0 ? Math.min(target, durationMs) : target;
}

/** A reported position this close to a jump's target means the player has arrived there (ms). */
export const ARRIVED_MS = 2000;

/**
 * The position to show on the seek bar: the target of a jump that is still on its way, so the bar
 * moves the moment the key is pressed, otherwise the position the player reports.
 */
export function shownPosition(input: { positionMs: number; pending: PendingSeek | null; now: number }): number {
  const { positionMs, pending, now } = input;
  const fresh = pending !== null && now - pending.at >= 0 && now - pending.at < SETTLE_MS;
  if (!fresh || Math.abs(positionMs - pending.targetMs) <= ARRIVED_MS) return positionMs;
  return pending.targetMs;
}
