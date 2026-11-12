import { SourceInfo } from '../types';

/** The resolution groups the viewer can filter the sources by. */
export type ResolutionTier = '1K' | '2K' | '4K';

export const RESOLUTION_TIERS: { id: ResolutionTier; hint: string }[] = [
  { id: '1K', hint: '1080p and lower' },
  { id: '2K', hint: '1440p' },
  { id: '4K', hint: '2160p (UHD) and higher' },
];

/** Qualities up to this belong to 1K (1080p and lower), up to `TWO_K_LIMIT` to 2K, the rest to 4K. */
const ONE_K_LIMIT = 1200;
const TWO_K_LIMIT = 2000;

/** The group a stream's vertical resolution belongs to; null when the provider did not say. */
export function resolutionTier(quality: number): ResolutionTier | null {
  if (!Number.isFinite(quality) || quality <= 0) return null;
  if (quality <= ONE_K_LIMIT) return '1K';
  if (quality <= TWO_K_LIMIT) return '2K';
  return '4K';
}

/**
 * The sources whose resolution is in any of the wanted groups. With nothing wanted every source
 * is kept. The source that is playing always stays listed.
 */
export function filterSourcesByResolution(sources: SourceInfo[], wanted: ResolutionTier[]): SourceInfo[] {
  if (wanted.length === 0) return sources;
  return sources.filter((source) => {
    const tier = resolutionTier(source.quality);
    return source.selected || (tier !== null && wanted.includes(tier));
  });
}
