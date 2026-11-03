import { HomeItem, WatchProgress } from '../types';

/** How much of the video was watched, 0 to 100. */
export function watchedPercent(item: WatchProgress): number {
  if (!Number.isFinite(item.percentage)) return 0;
  return Math.min(100, Math.max(0, Math.round(item.percentage * 100)));
}

/** Whether the title page of a Continue Watching entry is known (old entries may lack it). */
export function canOpenTitlePage(item: WatchProgress): boolean {
  return item.detail_url.trim() !== '';
}

/** The title page an entry opens: a card of Continue Watching shows the page, it does not play. */
export function titlePageItem(item: WatchProgress): HomeItem {
  return {
    id: item.detail_url,
    name: item.title,
    url: item.detail_url,
    poster_url: item.poster_url,
    provider: item.provider,
    media_type: null,
    year: null,
  };
}

/** What to tell the viewer when an entry has no title page to open. */
export function missingPageMessage(item: WatchProgress): string {
  return `"${item.title}" was saved without its title page. Open it from Search or Home and play it once to make it open from here.`;
}

/** A key that tells entries apart (the same media id can exist in two providers). */
export function continueKey(item: WatchProgress): string {
  return `${item.provider}\n${item.media_id}`;
}
