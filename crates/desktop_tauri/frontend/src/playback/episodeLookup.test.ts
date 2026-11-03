import { describe, expect, it } from 'vitest';
import { Episode, MediaDetails } from '../types';
import { findEpisodeForKey, movieAsEpisode, playbackTitle } from './episodeLookup';

const episode = (data: string): Episode => ({
  id: data,
  data,
  name: `Episode ${data}`,
  season: 1,
  episode: 1,
  poster_url: null,
  description: null,
});

const details = (overrides: Partial<MediaDetails>): MediaDetails => ({
  name: 'Show',
  url: 'https://site.test/show',
  provider: 'P',
  poster_url: null,
  plot: null,
  year: null,
  media_type: null,
  tags: [],
  episodes: [],
  data_url: null,
  background_url: null,
  rating: null,
  duration_minutes: null,
  cast: [],
  ...overrides,
});

describe('findEpisodeForKey', () => {
  it('returns the episode whose data matches', () => {
    const show = details({ episodes: [episode('a'), episode('b')] });
    expect(findEpisodeForKey(show, 'b')?.data).toBe('b');
  });

  it('returns the movie stream when the key is the movie data URL', () => {
    const movie = details({ data_url: 'https://cdn.test/movie' });
    expect(findEpisodeForKey(movie, 'https://cdn.test/movie')?.name).toBe('Show');
  });

  it('returns null when nothing matches', () => {
    expect(findEpisodeForKey(details({ episodes: [episode('a')] }), 'zzz')).toBeNull();
  });
});

describe('movieAsEpisode', () => {
  it('is null without a data URL', () => {
    expect(movieAsEpisode(details({}))).toBeNull();
  });
});

describe('playbackTitle', () => {
  it('does not repeat a movie name', () => {
    expect(playbackTitle('Movie', 'Movie')).toBe('Movie');
  });

  it('joins title and episode names', () => {
    expect(playbackTitle('Show', 'Pilot')).toBe('Show - Pilot');
  });
});
