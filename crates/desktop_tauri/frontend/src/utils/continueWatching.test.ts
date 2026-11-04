import { describe, expect, it } from 'vitest';
import {
  canOpenTitlePage,
  continueKey,
  episodeBadgeText,
  formatRemainingTime,
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
    expect(watchedPercent(entry({ position_ms: 204, duration_ms: 1000 }))).toBe(20);
    expect(watchedPercent(entry({ position_ms: 1200, duration_ms: 1000 }))).toBe(100);
    expect(watchedPercent(entry({ position_ms: 0, duration_ms: 0, percentage: 0.5 }))).toBe(50);
    expect(watchedPercent(entry({ position_ms: 0, duration_ms: 0, percentage: Number.NaN }))).toBe(0);
  });

  it('formats episode badge text correctly', () => {
    expect(episodeBadgeText({ season: 1, episode: 3 })).toBe('S1 E3');
    expect(episodeBadgeText({ season: null, episode: 4 })).toBe('E4');
    expect(episodeBadgeText({ season: undefined, episode: undefined })).toBeNull();
    expect(episodeBadgeText({ season: null, episode: null })).toBeNull();
  });

  it('formats remaining time into readable text', () => {
    expect(formatRemainingTime(0, 0)).toBe('0m left');
    expect(formatRemainingTime(60_000, 60_000)).toBe('0m left');
    expect(formatRemainingTime(0, 42 * 60_000)).toBe('42m left');
    expect(formatRemainingTime(0, 75 * 60_000)).toBe('1h 15m left');
    expect(formatRemainingTime(0, 120 * 60_000)).toBe('2h left');
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
