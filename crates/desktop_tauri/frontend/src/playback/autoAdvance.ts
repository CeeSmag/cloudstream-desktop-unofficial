import { PlaybackStatus } from '../types';

/** The video must have played to this share of its length (0 to 1) for its end to count as finished. */
export const FINISHED_FRACTION = 0.9;

/**
 * Whether the next episode should start now: the stream ended, there is a next episode, the viewer
 * wants that, and the video really reached its end (a stream cut off early must not skip the episode).
 */
export function shouldAutoAdvance(input: {
  status: Pick<PlaybackStatus, 'state' | 'position_ms' | 'duration_ms'>;
  hasNext: boolean;
  enabled: boolean;
}): boolean {
  const { status, hasNext, enabled } = input;
  if (!enabled || !hasNext || status.state !== 'Ended') return false;
  return status.duration_ms > 0 && status.position_ms >= status.duration_ms * FINISHED_FRACTION;
}
