import { describe, expect, it } from 'vitest';
import { canRefreshSources, refreshHint } from './sourceRefresh';

describe('refreshing the sources', () => {
  it('can be pressed once the video plays and no refresh is starting', () => {
    expect(canRefreshSources(false, false)).toBe(true);
  });

  it('cannot be pressed while the video is still opening', () => {
    expect(canRefreshSources(true, false)).toBe(false);
    expect(refreshHint(true, false)).toContain('once the video plays');
  });

  it('cannot be pressed twice at once', () => {
    expect(canRefreshSources(false, true)).toBe(false);
    expect(refreshHint(false, true)).toContain('Starting');
  });

  it('says the video keeps playing', () => {
    expect(refreshHint(false, false)).toContain('keeps playing');
  });
});
