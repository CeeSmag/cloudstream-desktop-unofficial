/**
 * How long a jump counts as "still on its way" (ms). The player's position is read about every
 * 0.8 s, so right after a jump it still shows the old place; a second key press in this time must
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
