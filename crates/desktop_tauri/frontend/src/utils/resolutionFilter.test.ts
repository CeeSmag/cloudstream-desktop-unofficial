import { describe, expect, it } from 'vitest';
import { SourceInfo } from '../types';
import { filterSourcesByResolution, resolutionTier } from './resolutionFilter';

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
  it('puts 1080p and lower in 1K, 1440p in 2K, 2160p in 4K', () => {
    expect(resolutionTier(360)).toBe('1K');
    expect(resolutionTier(1080)).toBe('1K');
    expect(resolutionTier(1440)).toBe('2K');
    expect(resolutionTier(2160)).toBe('4K');
    expect(resolutionTier(4320)).toBe('4K');
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

  it('keeps the sources in any wanted group', () => {
    expect(indexes(filterSourcesByResolution(sources, ['4K']))).toEqual([0]);
    expect(indexes(filterSourcesByResolution(sources, ['1K']))).toEqual([1, 4]);
    expect(indexes(filterSourcesByResolution(sources, ['2K', '4K']))).toEqual([0, 2]);
  });

  it('hides sources of unknown resolution while a group is wanted', () => {
    expect(indexes(filterSourcesByResolution(sources, ['1K', '2K', '4K']))).toEqual([0, 1, 2, 4]);
  });

  it('always keeps the source that is playing', () => {
    const playing = [source(0, 2160), source(1, 1080, true)];
    expect(indexes(filterSourcesByResolution(playing, ['4K']))).toEqual([0, 1]);
  });
});
