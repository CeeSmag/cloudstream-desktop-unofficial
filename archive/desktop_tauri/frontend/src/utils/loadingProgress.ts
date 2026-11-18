/** How a plugin's burst of requests is going, as the backend reports it. */
export interface PluginProgress {
  host: string;
  requests: number;
  limited: number;
  elapsed_s: number;
}

/** After this many seconds a title that is still loading gets an explanation. */
export const EXPLAIN_AFTER_S = 5;

/** "1 min 5 s" style time. */
export function formatWait(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return `${s} s`;
  return `${Math.floor(s / 60)} min ${s % 60} s`;
}

/** The lines shown under the spinner while a title loads: nothing extra at first, then why it takes long. */
export function loadingLines(waitedSeconds: number, progress: PluginProgress | null): string[] {
  if (waitedSeconds < EXPLAIN_AFTER_S) return [];
  const lines = [
    `Still loading (${formatWait(waitedSeconds)}). Long shows have many episode pages, and the site only allows a few requests at a time.`,
  ];
  if (progress && progress.requests > 0) {
    const waits =
      progress.limited > 0 ? `, ${progress.limited} of them waited for the site's request limit` : '';
    lines.push(`${progress.requests} pages fetched so far${waits}.`);
  }
  return lines;
}
