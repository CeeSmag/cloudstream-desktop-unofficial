import { WatchProgress } from '../types';
import { canOpenTitlePage, episodeBadgeText, formatRemainingTime } from './continueWatching';
import { markOf } from './episodeMarks';

/**
 * The title the Home banner offers to continue: the newest Continue Watching entry that has a
 * title page and is not already finished (80 percent or more watched).
 */
export function pickResumeHero(list: WatchProgress[]): WatchProgress | null {
  return list.find((item) => canOpenTitlePage(item) && markOf(item)?.kind !== 'watched') ?? null;
}

/** The short facts line of the banner, such as "S2 E5 · 54% watched · 22m left" or "S2 E6 · Up next". */
export function resumeFacts(item: WatchProgress): string {
  const mark = markOf(item);
  const parts: string[] = [];
  const badge = episodeBadgeText(item);
  if (badge) parts.push(badge);
  if (mark?.kind === 'partial') {
    parts.push(`${mark.percent}% watched`, formatRemainingTime(item.position_ms, item.duration_ms));
  } else {
    parts.push('Up next');
  }
  return parts.join(' · ');
}

/** The share watched (0 to 100) for the banner's bar; 0 when the episode was not started. */
export function resumePercent(item: WatchProgress): number {
  const mark = markOf(item);
  return mark?.kind === 'partial' ? mark.percent : 0;
}
