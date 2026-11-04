import { Episode, MediaDetails } from '../types';

/** Movies have no episode list; their single stream is described by `data_url`. */
export function movieAsEpisode(details: MediaDetails): Episode | null {
  if (!details.data_url) return null;
  return {
    id: details.data_url,
    data: details.data_url,
    name: details.name,
    season: null,
    episode: null,
    poster_url: details.poster_url,
    description: null,
  };
}

/** Finds what a saved progress entry points at: the matching episode, or the movie itself. */
export function findEpisodeForKey(details: MediaDetails, mediaKey: string): Episode | null {
  const episode = details.episodes.find((candidate) => candidate.data === mediaKey);
  if (episode) return episode;
  const movie = movieAsEpisode(details);
  return movie && movie.data === mediaKey ? movie : null;
}

/** The episodes in watching order: by season, then by number; episodes without a number go last. */
export function orderedEpisodes(episodes: Episode[]): Episode[] {
  const last = Number.MAX_SAFE_INTEGER;
  return [...episodes].sort(
    (a, b) => (a.season ?? 0) - (b.season ?? 0) || (a.episode ?? last) - (b.episode ?? last),
  );
}

/** The episode watched right after `current`, or null after the last one. */
export function findNextEpisode(episodes: Episode[], current: Episode): Episode | null {
  const ordered = orderedEpisodes(episodes);
  const index = ordered.findIndex((candidate) => candidate.data === current.data);
  return index >= 0 ? (ordered[index + 1] ?? null) : null;
}

/** The episode watched right before `current`, or null before the first one. */
export function findPreviousEpisode(episodes: Episode[], current: Episode): Episode | null {
  const ordered = orderedEpisodes(episodes);
  const index = ordered.findIndex((candidate) => candidate.data === current.data);
  return index > 0 ? (ordered[index - 1] ?? null) : null;
}

/** Title shown in the player; a movie is not repeated as "Name - Name". */
export function playbackTitle(titleName: string, episodeName: string): string {
  return episodeName === titleName ? titleName : `${titleName} - ${episodeName}`;
}
