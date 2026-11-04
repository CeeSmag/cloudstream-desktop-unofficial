import { WatchProgress } from '../types';

/** From this share of an episode on (0 to 1) it counts as watched. */
export const WATCHED_FRACTION = 0.8;

/** What the episode list shows for an episode the viewer has started. */
export type EpisodeMark = { kind: 'watched' } | { kind: 'partial'; percent: number };

/** The mark of one saved position; null when the episode was never started or its length is unknown. */
export function markOf(progress: Pick<WatchProgress, 'position_ms' | 'duration_ms'>): EpisodeMark | null {
  const { position_ms: position, duration_ms: duration } = progress;
  if (!(duration > 0) || !(position > 0)) return null;
  const fraction = Math.min(1, position / duration);
  if (fraction >= WATCHED_FRACTION) return { kind: 'watched' };
  return { kind: 'partial', percent: Math.round(fraction * 100) };
}

/** The marks of a show's saved positions, by the episode's playback key. */
export function marksByKey(list: WatchProgress[]): Map<string, EpisodeMark> {
  const marks = new Map<string, EpisodeMark>();
  for (const progress of list) {
    const mark = markOf(progress);
    if (mark) marks.set(progress.media_id, mark);
  }
  return marks;
}
