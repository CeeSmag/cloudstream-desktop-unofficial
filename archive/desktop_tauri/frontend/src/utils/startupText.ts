import { StartupStatus } from '../types';

/** The line under the logo: what start-up is doing right now. */
export function startupMessage(status: StartupStatus): string {
  if (status.ready) return 'Ready';
  if (status.total === 0) return 'Starting the extension runtime…';
  if (!status.current) return 'Finishing up…';
  return `Loading ${status.current} (${status.finished + 1} of ${status.total})`;
}

/** How much of the work is done, 0 to 100. */
export function startupPercent(status: StartupStatus): number {
  if (status.ready) return 100;
  if (status.total === 0) return 0;
  return Math.round((status.finished / status.total) * 100);
}
