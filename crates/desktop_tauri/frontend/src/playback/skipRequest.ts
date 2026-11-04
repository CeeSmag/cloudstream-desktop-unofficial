import { Episode, MediaDetails } from '../types';

/** What the backend needs to look up skip times for the video that is playing. */
export interface SkipRequest {
  imdb_id: string | null;
  tmdb_id: string | null;
  mal_id: string | null;
  media_type: string | null;
  is_movie: boolean;
  season: number | null;
  episode: number | null;
  duration_ms: number;
}

/** The lookup request for `episode` of `details`, once the length of the video is known. */
export function skipRequest(details: MediaDetails, episode: Episode, durationMs: number): SkipRequest {
  return {
    imdb_id: details.imdb_id,
    tmdb_id: details.tmdb_id,
    mal_id: details.mal_id,
    media_type: details.media_type,
    is_movie: details.episodes.length === 0,
    season: episode.season,
    episode: episode.episode,
    duration_ms: Math.max(0, Math.round(durationMs)),
  };
}

/** Whether a lookup makes sense: the title has an id the databases know and the length is known. */
export function canLookUpSkips(details: MediaDetails, durationMs: number): boolean {
  return durationMs > 0 && Boolean(details.imdb_id || details.tmdb_id || details.mal_id);
}
