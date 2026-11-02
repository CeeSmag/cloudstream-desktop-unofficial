import { describe, expect, it } from 'vitest';
import { clampVolume, DEFAULT_VOLUME, parseStoredVolume } from './volume';

describe('clampVolume', () => {
  it('keeps values inside 0-100 and rounds them', () => {
    expect(clampVolume(-5)).toBe(0);
    expect(clampVolume(250)).toBe(100);
    expect(clampVolume(42.6)).toBe(43);
  });

  it('falls back to the default for non-numbers', () => {
    expect(clampVolume(Number.NaN)).toBe(DEFAULT_VOLUME);
  });
});

describe('parseStoredVolume', () => {
  it('uses the default when nothing is stored or the value is damaged', () => {
    expect(parseStoredVolume(null)).toBe(DEFAULT_VOLUME);
    expect(parseStoredVolume('')).toBe(DEFAULT_VOLUME);
    expect(parseStoredVolume('loud')).toBe(DEFAULT_VOLUME);
  });

  it('reads a stored number', () => {
    expect(parseStoredVolume('35')).toBe(35);
  });
});
