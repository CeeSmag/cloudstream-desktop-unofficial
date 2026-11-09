import { describe, expect, it } from 'vitest';
import { FAILURES_PER_HOST, ImageTally, webImageUrl } from './imageLog';

describe('ImageTally', () => {
  it('reports the first failures of a site and then stops', () => {
    const tally = new ImageTally();
    const reported = Array.from({ length: 10 }, () => tally.shouldReport('i.site.test', false)).filter(Boolean);
    expect(reported).toHaveLength(FAILURES_PER_HOST);
  });

  it('counts each site and each outcome on its own', () => {
    const tally = new ImageTally();
    expect(tally.shouldReport('a.test', true)).toBe(true);
    expect(tally.shouldReport('a.test', true)).toBe(false);
    expect(tally.shouldReport('a.test', false)).toBe(true);
    expect(tally.shouldReport('b.test', true)).toBe(true);
  });
});

describe('webImageUrl', () => {
  it('accepts web addresses only', () => {
    expect(webImageUrl('https://i.site.test/a.jpg?x=1')?.host).toBe('i.site.test');
    expect(webImageUrl('data:image/png;base64,AAAA')).toBeNull();
    expect(webImageUrl('/local.png')).toBeNull();
    expect(webImageUrl('')).toBeNull();
  });
});
