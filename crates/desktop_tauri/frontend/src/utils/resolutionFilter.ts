import { SourceInfo } from '../types';

/** The resolution groups the viewer can filter the sources by. */
export type ResolutionTier = '1080p' | '2160p';

export const RESOLUTION_TIERS: { id: ResolutionTier; hint: string }[] = [
  { id: '1080p', hint: '1080p and lower' },
  { id: '2160p', hint: 'above 1080p: 1440p, 2160p (UHD) and higher' },
];

/** Qualities up to this belong to 1080p (1080p and lower); anything above to 2160p. */
const LOW_LIMIT = 1200;

/** The group a stream's vertical resolution (as the provider reports it) belongs to; null when unknown. */
export function resolutionTier(quality: number): ResolutionTier | null {
  if (!Number.isFinite(quality) || quality <= 0) return null;
  return quality <= LOW_LIMIT ? '1080p' : '2160p';
}

/**
 * The sources whose resolution is in any of the wanted groups. With nothing wanted every source
 * is kept. Nothing else stays listed, not even the source that is playing: it would look like a match.
 */
export function filterSourcesByResolution(sources: SourceInfo[], wanted: ResolutionTier[]): SourceInfo[] {
  if (wanted.length === 0) return sources;
  return sources.filter((source) => {
    const tier = resolutionTier(source.quality);
    return tier !== null && wanted.includes(tier);
  });
}

/**
 * The sources with a known file size first, biggest first; the ones without a size follow in
 * their old order (the sort is stable). The list is not changed.
 */
export function sortBySizeDescending(sources: SourceInfo[]): SourceInfo[] {
  const sized = sources.filter((source) => source.size_bytes !== null);
  const unsized = sources.filter((source) => source.size_bytes === null);
  sized.sort((a, b) => (b.size_bytes ?? 0) - (a.size_bytes ?? 0));
  return [...sized, ...unsized];
}

/** How many sources are in each group (sources of unknown resolution count in none). */
export function countByTier(sources: SourceInfo[]): Record<ResolutionTier, number> {
  const counts: Record<ResolutionTier, number> = { '1080p': 0, '2160p': 0 };
  for (const source of sources) {
    const tier = resolutionTier(source.quality);
    if (tier !== null) counts[tier] += 1;
  }
  return counts;
}
