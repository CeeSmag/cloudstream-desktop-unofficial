import { describe, expect, it } from 'vitest';
import { sourceRow } from './sourceRows';
import { SourceInfo } from '../types';

const source = (over: Partial<SourceInfo>): SourceInfo => ({
  index: 3,
  label: '2160p - Spider-Man.Brand.New.Day.2026.2160p.WEB-DL.mkv',
  selected: false,
  checking: false,
  kind: 'MKV',
  host: 'cdn.example.com',
  url: 'https://cdn.example.com/file.mkv?sig=abc',
  size_bytes: 1_400_000_000,
  ...over,
});

describe('sourceRow', () => {
  it('keeps the full name, shows the kind as a tag and the size as a flag', () => {
    const row = sourceRow(source({}));
    expect(row.label).toContain('WEB-DL.mkv');
    expect(row.tags).toEqual(['MKV']);
    expect(row.badge).toBe('1.3 GB');
  });

  it('does not show the host or the address', () => {
    const row = sourceRow(source({}));
    expect(row.tags).not.toContain('cdn.example.com');
    expect(row.detail).toBeUndefined();
  });

  it('has no size flag when the size is unknown', () => {
    expect(sourceRow(source({ size_bytes: null })).badge).toBeUndefined();
  });

  it('marks a stream that is still being checked', () => {
    expect(sourceRow(source({ checking: true })).tags).toContain('checking…');
  });
});
