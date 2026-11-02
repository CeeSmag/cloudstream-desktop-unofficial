import { describe, expect, it } from 'vitest';
import { formatTime, progressPercent, seekPositionFromClick } from './playerTime';

describe('formatTime', () => {
  it('shows minutes and padded seconds under an hour', () => {
    expect(formatTime(0)).toBe('0:00');
    expect(formatTime(65_000)).toBe('1:05');
  });

  it('adds hours with padded minutes from one hour up', () => {
    expect(formatTime(3_661_000)).toBe('1:01:01');
  });

  it('treats negative positions as zero', () => {
    expect(formatTime(-5_000)).toBe('0:00');
  });
});

describe('progressPercent', () => {
  it('is zero while the duration is unknown', () => {
    expect(progressPercent(5_000, 0)).toBe(0);
  });

  it('is clamped to the 0–100 range', () => {
    expect(progressPercent(50, 100)).toBe(50);
    expect(progressPercent(500, 100)).toBe(100);
    expect(progressPercent(-5, 100)).toBe(0);
  });
});

describe('seekPositionFromClick', () => {
  it('maps the middle of the bar to half the duration', () => {
    expect(seekPositionFromClick(150, 100, 100, 60_000)).toBe(30_000);
  });

  it('clamps clicks outside the bar', () => {
    expect(seekPositionFromClick(0, 100, 100, 60_000)).toBe(0);
    expect(seekPositionFromClick(999, 100, 100, 60_000)).toBe(60_000);
  });

  it('ignores a bar with no width', () => {
    expect(seekPositionFromClick(10, 0, 0, 60_000)).toBe(0);
  });
});
