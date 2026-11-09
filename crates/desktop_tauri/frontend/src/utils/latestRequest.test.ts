import { describe, expect, it } from 'vitest';
import { createLatestRequest } from './latestRequest';

describe('createLatestRequest', () => {
  it('a request is current until a newer one begins', () => {
    const latest = createLatestRequest();
    const first = latest.begin();
    expect(first()).toBe(true);
    const second = latest.begin();
    expect(first()).toBe(false);
    expect(second()).toBe(true);
  });

  it('an answer that arrives late is dropped, whatever the order the answers come in', () => {
    const latest = createLatestRequest();
    const shown: string[] = [];
    const slow = latest.begin();
    const fast = latest.begin();
    if (fast()) shown.push('fast provider');
    if (slow()) shown.push('slow provider');
    expect(shown).toEqual(['fast provider']);
  });
});
