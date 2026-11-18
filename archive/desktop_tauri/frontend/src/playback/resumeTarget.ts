import { api } from '../services/api';
import { Episode, MediaDetails, WatchProgress } from '../types';
import { canOpenTitlePage, missingPageMessage } from '../utils/continueWatching';
import { findEpisodeForKey } from './episodeLookup';

/** Loads the title of a Continue Watching entry and finds the episode (or movie) to play. */
export async function resumeTarget(progress: WatchProgress): Promise<{ details: MediaDetails; episode: Episode }> {
  if (!canOpenTitlePage(progress)) throw new Error(missingPageMessage(progress));
  const details = await api.getDetails(progress.provider, progress.detail_url);
  const episode = findEpisodeForKey(details, progress.media_id);
  if (!episode) throw new Error(`${progress.title} is no longer listed by ${progress.provider}.`);
  return { details, episode };
}
