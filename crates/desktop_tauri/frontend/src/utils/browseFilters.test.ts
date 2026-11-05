import { describe, expect, it } from 'vitest';
import {
  CURRENT_YEAR,
  GENRES,
  RATINGS,
  YEARS,
  clampPage,
  defaultBrowseFilters,
  formatRating,
  pickBestSearchMatch,
  selectSort,
  serializeFilterKey,
  toggleGenre,
} from './browseFilters';

describe('browseFilters utils', () => {
  it('includes all 19 required genres in the exact order', () => {
    expect(GENRES).toEqual([
      'Action',
      'Adventure',
      'Animation',
      'Comedy',
      'Crime',
      'Documentary',
      'Drama',
      'Family',
      'Fantasy',
      'History',
      'Horror',
      'Music',
      'Mystery',
      'Romance',
      'Science Fiction',
      'Thriller',
      'TV Movie',
      'War',
      'Western',
    ]);
  });

  it('includes all rating values from 0 to 10', () => {
    expect(RATINGS).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  });

  it('generates years in descending order starting with current year', () => {
    expect(YEARS[0]).toBe(CURRENT_YEAR);
    expect(YEARS[1]).toBe(CURRENT_YEAR - 1);
    expect(YEARS[YEARS.length - 1]).toBe(1970);
  });

  it('sets default tags to Movies, current year, Popular, and empty genres', () => {
    expect(defaultBrowseFilters.media_type).toBe('movie');
    expect(defaultBrowseFilters.year).toBe(CURRENT_YEAR);
    expect(defaultBrowseFilters.sort_by).toBe('popular');
    expect(defaultBrowseFilters.rating).toBeNull();
    expect(defaultBrowseFilters.genres).toEqual([]);
    expect(defaultBrowseFilters.page).toBe(1);
  });

  it('toggles multiple genres on and off cleanly', () => {
    const one = toggleGenre([], 'Action');
    expect(one).toEqual(['Action']);
    const two = toggleGenre(one, 'Comedy');
    expect(two).toEqual(['Action', 'Comedy']);
    const three = toggleGenre(two, 'Horror');
    expect(three).toEqual(['Action', 'Comedy', 'Horror']);
    const removed = toggleGenre(three, 'Comedy');
    expect(removed).toEqual(['Action', 'Horror']);
  });

  it('enforces mutual exclusion between popular and rating', () => {
    const ratingSort = selectSort('rating', null);
    expect(ratingSort.sort_by).toBe('rating');
    expect(ratingSort.rating).toBe(7);

    const popularSort = selectSort('popular', 8);
    expect(popularSort.sort_by).toBe('popular');
    expect(popularSort.rating).toBeNull();
  });

  it('clamps pagination correctly', () => {
    expect(clampPage(0, 10)).toBe(1);
    expect(clampPage(5, 10)).toBe(5);
    expect(clampPage(15, 10)).toBe(10);
    expect(clampPage(1, 0)).toBe(1);
  });

  it('formats rating nicely', () => {
    expect(formatRating(8.54)).toBe('8.5');
    expect(formatRating(0)).toBeNull();
    expect(formatRating(null)).toBeNull();
    expect(formatRating(undefined)).toBeNull();
  });

  it('serializes filter key deterministically including provider and multiple genres', () => {
    expect(
      serializeFilterKey(
        {
          media_type: 'tv',
          year: 2025,
          sort_by: 'rating',
          rating: 8,
          genres: ['Horror', 'Mystery'],
          page: 2,
        },
        'CineStream',
      ),
    ).toBe('CineStream|tv|2025|rating|8|Horror,Mystery|2');
  });

  it('picks best search match by title, media type, and year instead of blindly picking first', () => {
    const target = {
      id: 'tv:100',
      name: 'Fargo',
      poster_url: null,
      media_type: 'tv',
      year: 2014,
      rating: 8.9,
    };
    const results = [
      {
        id: '1',
        name: 'Fargo',
        poster_url: null,
        provider: 'TestProv',
        url: 'https://example.com/movie/fargo-1996',
        media_type: 'Movie',
        year: 1996,
      },
      {
        id: '2',
        name: 'Fargo',
        poster_url: null,
        provider: 'TestProv',
        url: 'https://example.com/tv/fargo-2014',
        media_type: 'TvSeries',
        year: 2014,
      },
    ];
    const match = pickBestSearchMatch(target, results);
    expect(match?.url).toBe('https://example.com/tv/fargo-2014');
  });
});
