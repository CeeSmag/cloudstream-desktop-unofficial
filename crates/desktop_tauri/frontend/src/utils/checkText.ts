import { CheckProgress } from '../types';

/** The headline of the panel shown while the streams of a video are checked. */
export function checkTitle(progress: CheckProgress): string {
  if (progress.total === 0) return 'Finding sources…';
  if (progress.skipping) return 'Starting as soon as a working source is found…';
  return 'Checking sources…';
}

/** The counts under the headline, empty while the sources are still being looked up. */
export function checkDetail(progress: CheckProgress): string {
  if (progress.total === 0) return '';
  return `${progress.checked} of ${progress.total} checked · ${progress.working} working`;
}

/** How much of the check is done, 0 to 100. */
export function checkPercent(progress: CheckProgress): number {
  if (progress.total === 0) return 0;
  return Math.min(100, Math.round((progress.checked / progress.total) * 100));
}
