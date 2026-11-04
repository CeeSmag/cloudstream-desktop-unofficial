import { describe, expect, it } from 'vitest';
import { createMemoryCache } from './detailsCache';

describe('createMemoryCache', () => {
  it('returns what was saved and nothing for an unknown key', () => {
    const cache = createMemoryCache<number>(3);
    cache.set('a', 1);
    expect(cache.get('a')).toBe(1);
    expect(cache.get('b')).toBeUndefined();
  });

  it('forgets the oldest entry once the limit is passed, and keeps exactly the limit', () => {
    const cache = createMemoryCache<number>(2);
    cache.set('a', 1);
    cache.set('b', 2);
    expect(cache.size()).toBe(2);
    cache.set('c', 3);
    expect(cache.size()).toBe(2);
    expect(cache.get('a')).toBeUndefined();
    expect(cache.get('b')).toBe(2);
    expect(cache.get('c')).toBe(3);
  });

  it('counts saving a key again as making it the newest and replaces its value', () => {
    const cache = createMemoryCache<number>(2);
    cache.set('a', 1);
    cache.set('b', 2);
    cache.set('a', 10);
    cache.set('c', 3);
    expect(cache.get('a')).toBe(10);
    expect(cache.get('b')).toBeUndefined();
  });
});
