import { CheckProgress } from '../types';

/** The headline of the panel shown while the streams of a video are checked. */
export function checkTitle(progress: CheckProgress): string {
  if (progress.total === 0) return 'Finding sources…';
  if (progress.skipping) return 'Starting playback…';
  if (progress.saved === 'trusted') return 'Opening with saved sources…';
  if (progress.saved === 'testing') return 'Testing saved sources…';
  if (progress.saved === 'outOfDate') return 'Saved sources are out of date, searching again…';
  return progress.discovering ? 'Finding and checking sources…' : 'Checking sources…';
}

/** The counts under the headline, empty while the sources are still being looked up. */
export function checkDetail(progress: CheckProgress): string {
  if (progress.total === 0) return '';
  const counts = `${progress.checked} of ${progress.total} checked · ${progress.working} working`;
  return progress.discovering ? `${counts} · still searching` : counts;
}

/** Whether the viewer may start now: a source is known to work and a skip is not on its way. */
export function canSkipCheck(progress: CheckProgress): boolean {
  return progress.working > 0 && !progress.skipping;
}

/** How much of the check is done, 0 to 100. */
export function checkPercent(progress: CheckProgress): number {
  if (progress.total === 0) return 0;
  return Math.min(100, Math.round((progress.checked / progress.total) * 100));
}
