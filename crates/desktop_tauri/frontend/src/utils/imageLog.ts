/** How many failed images per site are reported (the rest only add noise). */
export const FAILURES_PER_HOST = 3;
/** How many loaded images per site are reported: one is enough to show that the site's images work. */
export const LOADS_PER_HOST = 1;

/** Decides which image results are worth a log line: the first few per site. */
export class ImageTally {
  private failed = new Map<string, number>();
  private loaded = new Map<string, number>();

  /** Whether this result (for an image from `host`) should be reported. */
  shouldReport(host: string, loaded: boolean): boolean {
    const counts = loaded ? this.loaded : this.failed;
    const limit = loaded ? LOADS_PER_HOST : FAILURES_PER_HOST;
    const seen = counts.get(host) ?? 0;
    counts.set(host, seen + 1);
    return seen < limit;
  }
}

/** The address of a web image, or null for anything else (inline data, app files). */
export function webImageUrl(src: string | null | undefined): URL | null {
  if (!src) return null;
  try {
    const url = new URL(src);
    return url.protocol === 'http:' || url.protocol === 'https:' ? url : null;
  } catch {
    return null;
  }
}

/**
 * Watches every image of the page (load and error events do not bubble, so they are caught on the way
 * down) and reports the first few per site, so a blocked image shows up in the logs with its site.
 */
export function installImageLogging(report: (url: string, loaded: boolean) => void): void {
  const tally = new ImageTally();
  const seen = (loaded: boolean) => (event: Event) => {
    const target = event.target;
    if (!(target instanceof HTMLImageElement)) return;
    const url = webImageUrl(target.currentSrc || target.src);
    if (url && tally.shouldReport(url.host, loaded)) report(url.href, loaded);
  };
  document.addEventListener('error', seen(false), true);
  document.addEventListener('load', seen(true), true);
}
