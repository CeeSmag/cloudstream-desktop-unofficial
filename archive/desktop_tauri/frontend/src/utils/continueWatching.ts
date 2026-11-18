import { HomeItem, WatchProgress } from '../types';

/** How much of the video was watched, 0 to 100. */
export function watchedPercent(item: WatchProgress): number {
  if (item.duration_ms > 0) {
    return Math.min(100, Math.max(0, Math.round((item.position_ms / item.duration_ms) * 100)));
  }
  if (!Number.isFinite(item.percentage)) return 0;
  return Math.min(100, Math.max(0, Math.round(item.percentage * 100)));
}

/** Formats season and episode into a badge label (e.g. "S1 E3", "E3", or null for movies). */
export function episodeBadgeText(item: { season?: number | null; episode?: number | null }): string | null {
  const { season, episode } = item;
  if (season != null && episode != null) {
    return `S${season} E${episode}`;
  }
  if (episode != null) {
    return `E${episode}`;
  }
  return null;
}

/** Formats remaining playback time in milliseconds into human readable text (e.g. "42m left", "1h 15m left"). */
export function formatRemainingTime(positionMs: number, durationMs: number): string {
  if (durationMs <= 0 || positionMs >= durationMs) {
    return '0m left';
  }
  const remainingMs = Math.max(0, durationMs - positionMs);
  const totalMinutes = Math.max(1, Math.round(remainingMs / 60000));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours > 0 && minutes > 0) {
    return `${hours}h ${minutes}m left`;
  }
  if (hours > 0) {
    return `${hours}h left`;
  }
  return `${minutes}m left`;
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

/** A key that tells entries apart (by provider and detail URL or media key). */
export function continueKey(item: WatchProgress): string {
  const id = item.detail_url || item.media_id;
  return `${item.provider}\n${id}`;
}
