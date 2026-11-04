import { SkipSegment } from '../types';

/** A credits segment that ends this close to the end of the video means "the episode is over". */
export const NEXT_EPISODE_WINDOW_MS = 20_000;
/** Without any skip times, the Next episode button shows from this share of the video on (0 to 1). */
export const NEXT_BUTTON_FRACTION = 0.9;

/** The button over the video: jump somewhere, or go to the next episode. */
export type SkipPrompt = { label: string; action: 'seek'; toMs: number } | { label: string; action: 'next' };

const LABELS: Record<SkipSegment['kind'], string> = {
  intro: 'Skip Intro',
  recap: 'Skip Recap',
  credits: 'Skip Credits',
  preview: 'Skip Preview',
};

/**
 * What to offer at `positionMs`: inside a known segment its skip button; inside the credits that
 * run to the end of the video, Next episode (when there is one); with no segment, Next episode
 * from 90 percent on.
 */
export function skipPrompt(input: {
  segments: SkipSegment[];
  positionMs: number;
  durationMs: number;
  hasNext: boolean;
}): SkipPrompt | null {
  const { segments, positionMs, durationMs, hasNext } = input;
  const active = segments.find((s) => positionMs >= s.start_ms && positionMs < s.end_ms);
  if (active) {
    const endsTheEpisode =
      (active.kind === 'credits' || active.kind === 'preview') &&
      durationMs > 0 &&
      durationMs - active.end_ms < NEXT_EPISODE_WINDOW_MS;
    if (hasNext && endsTheEpisode) return { label: 'Next episode', action: 'next' };
    return { label: LABELS[active.kind], action: 'seek', toMs: active.end_ms };
  }
  if (hasNext && durationMs > 0 && positionMs >= durationMs * NEXT_BUTTON_FRACTION) {
    return { label: 'Next episode', action: 'next' };
  }
  return null;
}
