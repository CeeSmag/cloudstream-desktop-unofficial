import { describe, expect, it } from 'vitest';
import { seekText, shorten, sourceText, trackText, volumeText } from './feedbackText';

describe('feedback texts', () => {
  it('shows jumps with their direction', () => {
    expect(seekText(5000)).toBe('+5 s');
    expect(seekText(-10000)).toBe('−10 s');
  });

  it('shows the volume, and muted at zero', () => {
    expect(volumeText(55)).toBe('Volume 55%');
    expect(volumeText(0)).toBe('Muted');
  });

  it('names the chosen track, or says subtitles are off', () => {
    expect(trackText('subtitle', 'English')).toBe('Subtitles: English');
    expect(trackText('subtitle', null)).toBe('Subtitles off');
    expect(trackText('audio', 'Hindi')).toBe('Audio: Hindi');
  });

  it('cuts very long source names', () => {
    const text = sourceText('x'.repeat(200));
    expect(text.length).toBe(60);
    expect(text.endsWith('…')).toBe(true);
    expect(shorten('short')).toBe('short');
  });
});
