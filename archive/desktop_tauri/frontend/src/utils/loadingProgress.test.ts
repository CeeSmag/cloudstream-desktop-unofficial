import { describe, expect, it } from 'vitest';
import { formatWait, loadingLines } from './loadingProgress';

const progress = { host: 'animepahe.pw', requests: 24, limited: 6, elapsed_s: 30 };

describe('formatWait', () => {
  it('shows seconds, then minutes', () => {
    expect(formatWait(7)).toBe('7 s');
    expect(formatWait(65)).toBe('1 min 5 s');
    expect(formatWait(-3)).toBe('0 s');
  });
});

describe('loadingLines', () => {
  it('says nothing while loading is quick', () => {
    expect(loadingLines(2, null)).toEqual([]);
  });

  it('explains a slow load, then adds the progress when there is some', () => {
    expect(loadingLines(8, null)).toHaveLength(1);
    const lines = loadingLines(40, progress);
    expect(lines).toHaveLength(2);
    expect(lines[1]).toContain('24 pages');
    expect(lines[1]).toContain('6 of them waited');
  });

  it('does not mention waits when the site never refused', () => {
    expect(loadingLines(40, { ...progress, limited: 0 })[1]).not.toContain('waited');
  });
});
