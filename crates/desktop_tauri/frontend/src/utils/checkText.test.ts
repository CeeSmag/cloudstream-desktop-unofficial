import { describe, expect, it } from 'vitest';
import { canSkipCheck, checkDetail, checkPercent, checkTitle } from './checkText';
import { CheckProgress } from '../types';

const progress = (over: Partial<CheckProgress>): CheckProgress => ({
  checked: 0,
  total: 0,
  working: 0,
  skipping: false,
  discovering: false,
  saved: 'none',
  ...over,
});

describe('check text', () => {
  it('says the sources are being found until their number is known', () => {
    expect(checkTitle(progress({}))).toBe('Finding sources…');
    expect(checkDetail(progress({}))).toBe('');
  });

  it('counts checked and working sources', () => {
    const now = progress({ checked: 23, total: 130, working: 12 });
    expect(checkTitle({ ...now, working: 0 })).toBe('Checking sources…');
    expect(checkDetail(now)).toBe('23 of 130 checked · 12 working');
    expect(checkPercent(now)).toBe(18);
  });

  it('keeps checking, however many sources already work, until the viewer skips', () => {
    const now = progress({ checked: 2, total: 9, working: 1, discovering: true });
    expect(checkTitle(now)).toBe('Finding and checking sources…');
    expect(checkDetail(now)).toBe('2 of 9 checked · 1 working · still searching');
    expect(checkTitle({ ...now, discovering: false })).toBe('Checking sources…');
  });

  it('says playback is starting once the viewer has skipped', () => {
    expect(checkTitle(progress({ total: 5, working: 1, skipping: true }))).toBe('Starting playback…');
  });

  it('tells what happens with sources saved from an earlier time', () => {
    const base = progress({ total: 3, checked: 1, discovering: true });
    expect(checkTitle({ ...base, saved: 'trusted' })).toBe('Opening with saved sources…');
    expect(checkTitle({ ...base, saved: 'testing' })).toBe('Testing saved sources…');
    expect(checkTitle({ ...base, saved: 'outOfDate' })).toBe(
      'Saved sources are out of date, searching again…',
    );
    expect(checkTitle({ ...base, saved: 'none' })).toBe('Finding and checking sources…');
  });

  it('says playback is starting once the viewer skipped, whatever the saved sources are', () => {
    expect(checkTitle(progress({ total: 3, working: 1, skipping: true, saved: 'testing' }))).toBe(
      'Starting playback…',
    );
  });

  it('offers the skip only when a source works and the skip is not already on its way', () => {
    expect(canSkipCheck(progress({}))).toBe(false);
    expect(canSkipCheck(progress({ total: 5, checked: 5, working: 0 }))).toBe(false);
    expect(canSkipCheck(progress({ total: 5, checked: 1, working: 1 }))).toBe(true);
    expect(canSkipCheck(progress({ total: 5, checked: 1, working: 1, skipping: true }))).toBe(false);
  });
});
