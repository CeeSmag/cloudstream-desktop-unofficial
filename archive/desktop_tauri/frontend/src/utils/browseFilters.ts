import { BrowseFilters, BrowseItem, SearchItem } from '../types';

export const GENRES = [
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
] as const;

export const RATINGS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10] as const;

export const CURRENT_YEAR = new Date().getFullYear();
export const OLDEST_YEAR = 1970;
export const YEARS: number[] = Array.from(
  { length: CURRENT_YEAR - OLDEST_YEAR + 1 },
  (_, i) => CURRENT_YEAR - i,
);

export const defaultBrowseFilters: BrowseFilters = {
  media_type: 'movie',
  year: CURRENT_YEAR,
  sort_by: 'popular',
  rating: null,
  genres: [],
  page: 1,
};

/** Toggles a genre in or out of the selected genres array. */
export function toggleGenre(current: string[], target: string): string[] {
  if (current.includes(target)) {
    return current.filter((g) => g !== target);
  }
  return [...current, target];
}

/** Handles mutual exclusion between Popular and Rating sort. */
export function selectSort(
  newSort: 'popular' | 'rating',
  currentRating: number | null,
): { sort_by: 'popular' | 'rating'; rating: number | null } {
  if (newSort === 'rating') {
    return { sort_by: 'rating', rating: currentRating ?? 7 };
  }
  return { sort_by: 'popular', rating: null };
}

/** Clamps a page index to valid range [1, totalPages]. */
export function clampPage(page: number, totalPages: number): number {
  if (totalPages <= 0) return 1;
  return Math.min(Math.max(1, page), totalPages);
}

/** Formats rating score to 1 decimal place, e.g. "8.5". */
export function formatRating(rating: number | null | undefined): string | null {
  if (rating == null || rating <= 0 || Number.isNaN(rating)) return null;
  return rating.toFixed(1);
}

/** Produces a deterministic cache key for a BrowseFilters + active provider state. */
export function serializeFilterKey(f: BrowseFilters, provider = ''): string {
  const genresPart = f.genres.length > 0 ? f.genres.join(',') : 'all';
  return `${provider}|${f.media_type}|${f.year ?? 'all'}|${f.sort_by}|${f.rating ?? 'none'}|${genresPart}|${f.page}`;
}

function normalizeTitle(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9\s]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function matchesMediaType(itemType: string, candidateType?: string | null): number {
  if (!candidateType) return 0;
  const t = candidateType.toLowerCase();
  const isCandidateTv =
    t.includes('tv') || t.includes('series') || t.includes('anime') || t.includes('ova');
  const isCandidateMovie = t.includes('movie') || t.includes('film');
  if (itemType.toLowerCase() === 'tv') {
    return isCandidateTv ? 30 : isCandidateMovie ? -20 : 0;
  }
  return isCandidateMovie ? 30 : isCandidateTv ? -20 : 0;
}

function scoreSearchCandidate(item: BrowseItem, targetNorm: string, candidate: SearchItem): number {
  const candNorm = normalizeTitle(candidate.name);
  let score = 0;
  if (candNorm === targetNorm) {
    score += 100;
  } else if (candNorm.startsWith(targetNorm) || targetNorm.startsWith(candNorm)) {
    score += 40;
  }
  score += matchesMediaType(item.media_type, candidate.media_type);
  if (item.year && candidate.year) {
    const diff = Math.abs(item.year - candidate.year);
    score += diff === 0 ? 25 : diff === 1 ? 10 : -15;
  }
  return score;
}

/** Picks the best provider search result matching a BrowseItem by title, media type, and year. */
export function pickBestSearchMatch<T extends SearchItem>(
  item: BrowseItem,
  results: T[],
): T | null {
  if (results.length === 0) return null;
  const targetNorm = normalizeTitle(item.name);
  let best = results[0];
  let bestScore = scoreSearchCandidate(item, targetNorm, best);

  for (let i = 1; i < results.length; i += 1) {
    const candidate = results[i];
    const score = scoreSearchCandidate(item, targetNorm, candidate);
    if (score > bestScore) {
      best = candidate;
      bestScore = score;
    }
  }
  return best;
}
