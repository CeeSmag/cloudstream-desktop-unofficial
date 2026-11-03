import { describe, expect, it } from 'vitest';
import {
  canOpenTitlePage,
  continueKey,
  missingPageMessage,
  titlePageItem,
  watchedPercent,
} from './continueWatching';
import { WatchProgress } from '../types';

const entry = (over: Partial<WatchProgress> = {}): WatchProgress => ({
  media_id: 'ep-1',
  provider: 'CineStream',
  detail_url: 'https://site.test/show',
  poster_url: 'https://img.test/p.jpg',
  title: 'Show - Episode 1',
  position_ms: 600,
  duration_ms: 1000,
  percentage: 0.6,
  updated_at: 1,
  ...over,
});

describe('continue watching', () => {
  it('rounds the watched share and keeps it between 0 and 100', () => {
    expect(watchedPercent(entry({ percentage: 0.204 }))).toBe(20);
    expect(watchedPercent(entry({ percentage: 1.7 }))).toBe(100);
    expect(watchedPercent(entry({ percentage: -1 }))).toBe(0);
    expect(watchedPercent(entry({ percentage: Number.NaN }))).toBe(0);
  });

  it('opens the title page, not the video, from the saved page address', () => {
    const page = titlePageItem(entry());
    expect(page.url).toBe('https://site.test/show');
    expect(page.provider).toBe('CineStream');
    expect(page.name).toBe('Show - Episode 1');
    expect(page.poster_url).toBe('https://img.test/p.jpg');
  });

  it('knows when an old entry has no title page to open', () => {
    expect(canOpenTitlePage(entry())).toBe(true);
    expect(canOpenTitlePage(entry({ detail_url: '' }))).toBe(false);
    expect(canOpenTitlePage(entry({ detail_url: '   ' }))).toBe(false);
    expect(missingPageMessage(entry({ detail_url: '' }))).toContain('Show - Episode 1');
  });

  it('tells entries of two providers with the same media id apart', () => {
    expect(continueKey(entry())).not.toBe(continueKey(entry({ provider: 'Other' })));
    expect(continueKey(entry())).toBe(continueKey(entry()));
  });
});
