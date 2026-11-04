import { describe, expect, it } from 'vitest';
import { canLookUpSkips, skipRequest } from './skipRequest';
import { Episode, MediaDetails } from '../types';

const episode: Episode = {
  id: 'e',
  data: 'e',
  name: 'E',
  season: 2,
  episode: 5,
  poster_url: null,
  description: null,
};
const details = (overrides: Partial<MediaDetails> = {}): MediaDetails => ({
  name: 'Show',
  url: 'u',
  provider: 'P',
  media_type: 'TvSeries',
  data_url: null,
  poster_url: null,
  year: null,
  plot: null,
  tags: [],
  episodes: [episode],
  background_url: null,
  rating: null,
  duration_minutes: null,
  cast: [],
  imdb_id: 'tt1234567',
  tmdb_id: null,
  mal_id: null,
  ...overrides,
});

describe('skipRequest', () => {
  it('carries the ids, the kind of title and the episode', () => {
    expect(skipRequest(details(), episode, 1_440_000.4)).toEqual({
      imdb_id: 'tt1234567',
      tmdb_id: null,
      mal_id: null,
      media_type: 'TvSeries',
      is_movie: false,
      season: 2,
      episode: 5,
      duration_ms: 1_440_000,
    });
  });

  it('calls a title without episodes a film', () => {
    expect(skipRequest(details({ episodes: [] }), episode, 5).is_movie).toBe(true);
  });
});

describe('canLookUpSkips', () => {
  it('needs the length and at least one id', () => {
    expect(canLookUpSkips(details(), 1000)).toBe(true);
    expect(canLookUpSkips(details(), 0)).toBe(false);
    expect(canLookUpSkips(details({ imdb_id: null }), 1000)).toBe(false);
    expect(canLookUpSkips(details({ imdb_id: null, mal_id: '21' }), 1000)).toBe(true);
  });
});
