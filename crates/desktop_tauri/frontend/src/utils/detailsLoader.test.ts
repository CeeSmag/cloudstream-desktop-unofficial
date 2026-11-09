import { describe, expect, it, vi } from 'vitest';
import { createDetailsLoader } from './detailsLoader';

function setup(ttlMs = 1000, limit = 2) {
  let clock = 0;
  const fetcher = vi.fn((provider: string, url: string) => Promise.resolve(`${provider}:${url}`));
  const note = vi.fn();
  const load = createDetailsLoader(fetcher, { ttlMs, limit, now: () => clock, note });
  return { load, fetcher, note, advance: (ms: number) => (clock += ms) };
}

describe('createDetailsLoader', () => {
  it('answers a title loaded a moment ago from memory', async () => {
    const { load, fetcher, note } = setup();
    await load('p', 'a');
    await load('p', 'a');
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(note).toHaveBeenCalledWith(expect.stringContaining('from memory'));
  });

  it('lets a second request join a load that is still running', async () => {
    const { load, fetcher, note } = setup();
    const [first, second] = await Promise.all([load('p', 'a'), load('p', 'a')]);
    expect(first).toBe(second);
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(note).toHaveBeenCalledWith(expect.stringContaining('joining'));
  });

  it('loads again once the memory is too old', async () => {
    const { load, fetcher, advance } = setup(1000);
    await load('p', 'a');
    advance(1001);
    await load('p', 'a');
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it('never remembers a failure, so the next ask tries again', async () => {
    let fail = true;
    const fetcher = vi.fn(() => (fail ? Promise.reject(new Error('down')) : Promise.resolve('ok')));
    const load = createDetailsLoader(fetcher, { ttlMs: 1000, limit: 2, now: () => 0, note: () => undefined });
    await expect(load('p', 'a')).rejects.toThrow('down');
    fail = false;
    await expect(load('p', 'a')).resolves.toBe('ok');
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it('forgets the oldest title past the limit', async () => {
    const { load, fetcher } = setup(1000, 2);
    await load('p', 'a');
    await load('p', 'b');
    await load('p', 'c');
    await load('p', 'a');
    expect(fetcher).toHaveBeenCalledTimes(4);
  });

  it('keeps titles of different providers apart', async () => {
    const { load, fetcher } = setup();
    await load('one', 'a');
    await load('two', 'a');
    expect(fetcher).toHaveBeenCalledTimes(2);
  });
});
