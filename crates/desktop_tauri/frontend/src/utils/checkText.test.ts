import { describe, expect, it } from 'vitest';
import { checkDetail, checkPercent, checkTitle } from './checkText';
import { CheckProgress } from '../types';

const progress = (over: Partial<CheckProgress>): CheckProgress => ({
  checked: 0,
  total: 0,
  working: 0,
  skipping: false,
  discovering: false,
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

  it('says playback is starting once a source works, and that the search goes on', () => {
    const now = progress({ checked: 2, total: 9, working: 1, discovering: true });
    expect(checkTitle(now)).toBe('Starting playback…');
    expect(checkDetail(now)).toBe('2 of 9 checked · 1 working · still searching');
  });

  it('tells that a skipped check waits for the first working source', () => {
    expect(checkTitle(progress({ total: 5, skipping: true }))).toContain('working source');
  });
});
