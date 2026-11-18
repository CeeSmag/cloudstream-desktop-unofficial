import { describe, expect, it } from 'vitest';
import { mayFetchHome, POST_READY_STAGGER_MS } from './homeFetchGate';

describe('mayFetchHome', () => {
  it('blocks home work until start-up is ready', () => {
    expect(mayFetchHome(false)).toBe(false);
    expect(mayFetchHome(true)).toBe(true);
  });

  it('staggers post-ready work by dozens of seconds', () => {
    expect(POST_READY_STAGGER_MS).toBeGreaterThanOrEqual(30_000);
    expect(POST_READY_STAGGER_MS).toBeLessThanOrEqual(60_000);
  });
});
