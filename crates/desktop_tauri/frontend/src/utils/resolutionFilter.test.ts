import { describe, expect, it } from 'vitest';
import { SourceInfo } from '../types';
import { countByTier, filterSourcesByResolution, resolutionTier } from './resolutionFilter';

const source = (index: number, quality: number, selected = false): SourceInfo => ({
  index,
  label: `S${index}`,
  selected,
  checking: false,
  kind: 'HLS',
  host: 'h',
  url: `https://h/${index}`,
  size_bytes: null,
  subtitle_languages: [],
  quality,
});

const indexes = (sources: SourceInfo[]) => sources.map((s) => s.index);

describe('resolutionTier', () => {
  it('puts 1080p and lower in 1080p, and anything above in 2160p', () => {
    expect(resolutionTier(360)).toBe('1080p');
    expect(resolutionTier(1080)).toBe('1080p');
    expect(resolutionTier(1440)).toBe('2160p');
    expect(resolutionTier(2160)).toBe('2160p');
    expect(resolutionTier(4320)).toBe('2160p');
  });

  it('has no group when the provider gave no resolution', () => {
    expect(resolutionTier(0)).toBeNull();
    expect(resolutionTier(-1)).toBeNull();
    expect(resolutionTier(Number.NaN)).toBeNull();
  });
});

describe('filterSourcesByResolution', () => {
  const sources = [source(0, 2160), source(1, 1080), source(2, 1440), source(3, 0), source(4, 720)];

  it('keeps every source when nothing is wanted', () => {
    expect(indexes(filterSourcesByResolution(sources, []))).toEqual([0, 1, 2, 3, 4]);
  });

  it('keeps the sources in a wanted group', () => {
    expect(indexes(filterSourcesByResolution(sources, ['2160p']))).toEqual([0, 2]);
    expect(indexes(filterSourcesByResolution(sources, ['1080p']))).toEqual([1, 4]);
  });

  it('hides sources of unknown resolution while a group is wanted', () => {
    expect(indexes(filterSourcesByResolution(sources, ['1080p', '2160p']))).toEqual([0, 1, 2, 4]);
  });

  it('does not keep the playing source when it is not in a wanted group', () => {
    const playing = [source(0, 2160), source(1, 1080, true)];
    expect(indexes(filterSourcesByResolution(playing, ['2160p']))).toEqual([0]);
  });
});

describe('countByTier', () => {
  it('counts the sources in each group and ignores unknown resolutions', () => {
    const sources = [source(0, 2160), source(1, 1080), source(2, 1080), source(3, 0), source(4, 762)];
    expect(countByTier(sources)).toEqual({ '1080p': 3, '2160p': 1 });
  });
});
