import { describe, expect, it } from 'vitest';
import { Episode, MediaDetails } from '../types';
import { findEpisodeForKey, findNextEpisode, movieAsEpisode, playbackTitle } from './episodeLookup';

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

describe('findNextEpisode', () => {
  it('returns the next episode in order', () => {
    const ep1 = episode('ep1');
    const ep2 = episode('ep2');
    const ep3 = episode('ep3');
    expect(findNextEpisode([ep1, ep2, ep3], ep1)?.data).toBe('ep2');
    expect(findNextEpisode([ep1, ep2, ep3], ep2)?.data).toBe('ep3');
  });

  it('returns null when current is the last episode', () => {
    const ep1 = episode('ep1');
    const ep2 = episode('ep2');
    expect(findNextEpisode([ep1, ep2], ep2)).toBeNull();
  });

  it('returns null when current is not in the list', () => {
    const ep1 = episode('ep1');
    const other = episode('other');
    expect(findNextEpisode([ep1], other)).toBeNull();
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

