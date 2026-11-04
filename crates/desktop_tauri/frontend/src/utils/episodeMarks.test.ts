import { describe, expect, it } from 'vitest';
import { marksByKey, markOf } from './episodeMarks';
import { WatchProgress } from '../types';

const at = (position_ms: number, duration_ms = 1000) => ({ position_ms, duration_ms });

describe('markOf', () => {
  it('gives nothing for an episode that was never started or has no known length', () => {
    expect(markOf(at(0))).toBeNull();
    expect(markOf(at(500, 0))).toBeNull();
    expect(markOf(at(-5))).toBeNull();
  });

  it('shows the share watched for an episode in the middle', () => {
    expect(markOf(at(250))).toEqual({ kind: 'partial', percent: 25 });
    expect(markOf(at(1))).toEqual({ kind: 'partial', percent: 0 });
  });

  it('counts exactly 80 percent as watched and 79.9 percent as not yet', () => {
    expect(markOf(at(799))).toEqual({ kind: 'partial', percent: 80 });
    expect(markOf(at(800))).toEqual({ kind: 'watched' });
    expect(markOf(at(1000))).toEqual({ kind: 'watched' });
  });

  it('treats a position past the end as watched', () => {
    expect(markOf(at(5000))).toEqual({ kind: 'watched' });
  });
});

describe('marksByKey', () => {
  const entry = (media_id: string, position_ms: number): WatchProgress => ({
    media_id,
    provider: 'P',
    detail_url: 'u',
    poster_url: null,
    title: 't',
    position_ms,
    duration_ms: 1000,
    percentage: 0,
    updated_at: 0,
  });

  it('maps each started episode by its key and leaves the others out', () => {
    const marks = marksByKey([entry('a', 900), entry('b', 100), entry('c', 0)]);
    expect([...marks.keys()]).toEqual(['a', 'b']);
    expect(marks.get('a')).toEqual({ kind: 'watched' });
  });
});
