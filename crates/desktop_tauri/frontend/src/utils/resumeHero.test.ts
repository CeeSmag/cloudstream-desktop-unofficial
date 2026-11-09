import { describe, expect, it } from 'vitest';
import { pickResumeHero, resumeFacts, resumePercent } from './resumeHero';
import { WatchProgress } from '../types';

const entry = (overrides: Partial<WatchProgress> = {}): WatchProgress => ({
  media_id: 'ep',
  provider: 'P',
  detail_url: 'https://site.test/show',
  poster_url: null,
  title: 'Show',
  position_ms: 540_000,
  duration_ms: 1_000_000,
  percentage: 0.54,
  updated_at: 1,
  ...overrides,
});

describe('pickResumeHero', () => {
  it('takes the first entry, which is the newest', () => {
    const list = [entry({ title: 'A' }), entry({ title: 'B', detail_url: 'other' })];
    expect(pickResumeHero(list, 'P')?.title).toBe('A');
  });

  it('skips an entry that is watched (80 percent or more) and one without a title page', () => {
    const list = [
      entry({ title: 'done', position_ms: 800_000 }),
      entry({ title: 'old', detail_url: '' }),
      entry({ title: 'next' }),
    ];
    expect(pickResumeHero(list, 'P')?.title).toBe('next');
  });

  it('keeps an entry at 79.9 percent and drops one at exactly 80', () => {
    expect(pickResumeHero([entry({ position_ms: 799_000 })], 'P')).not.toBeNull();
    expect(pickResumeHero([entry({ position_ms: 800_000 })], 'P')).toBeNull();
  });

  it('keeps a not-started next episode and gives nothing for an empty list', () => {
    expect(pickResumeHero([entry({ position_ms: 0, duration_ms: 0 })], 'P')).not.toBeNull();
    expect(pickResumeHero([], 'P')).toBeNull();
  });
});

describe('pickResumeHero per provider', () => {
  const list = [
    entry({ title: 'One Piece', provider: 'AnimePahe' }),
    entry({ title: 'Other', provider: 'AllWish', detail_url: 'https://other.test/x' }),
  ];

  it("offers only the selected provider's own last title", () => {
    expect(pickResumeHero(list, 'AnimePahe')?.title).toBe('One Piece');
    expect(pickResumeHero(list, 'AllWish')?.title).toBe('Other');
  });

  it('gives nothing for a provider with nothing in progress, even when others have titles', () => {
    expect(pickResumeHero(list, 'CineStream')).toBeNull();
    expect(pickResumeHero(list, '')).toBeNull();
  });

  it("skips a finished title of the provider instead of falling back to another provider's", () => {
    const finished = [entry({ title: 'done', provider: 'AnimePahe', position_ms: 900_000 }), list[1]];
    expect(pickResumeHero(finished, 'AnimePahe')).toBeNull();
  });
});

describe('resumeFacts', () => {
  it('names the episode, the share watched and the time left', () => {
    const facts = resumeFacts(entry({ season: 2, episode: 5, position_ms: 540_000, duration_ms: 3_600_000 }));
    expect(facts).toBe('S2 E5 · 15% watched · 51m left');
  });

  it('says Up next for an episode that was not started, and leaves the badge out for a movie', () => {
    expect(resumeFacts(entry({ season: 2, episode: 6, position_ms: 0, duration_ms: 0 }))).toBe('S2 E6 · Up next');
    expect(resumeFacts(entry({ position_ms: 500_000 }))).toBe('50% watched · 8m left');
  });
});

describe('resumePercent', () => {
  it('is the share watched, or 0 when not started', () => {
    expect(resumePercent(entry())).toBe(54);
    expect(resumePercent(entry({ position_ms: 0, duration_ms: 0 }))).toBe(0);
  });
});
