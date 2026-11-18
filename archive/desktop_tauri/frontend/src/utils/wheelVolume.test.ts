import { describe, expect, it } from 'vitest';
import { wheelVolumeDelta } from './wheelVolume';

describe('wheelVolumeDelta', () => {
  it('raises the volume by 5% for one notch up and lowers it for one notch down', () => {
    expect(wheelVolumeDelta(-100, 0)).toEqual({ delta: 5, carry: 0 });
    expect(wheelVolumeDelta(100, 0)).toEqual({ delta: -5, carry: 0 });
  });

  it('adds up the small steps of a touchpad instead of losing them', () => {
    let carry = 0;
    let total = 0;
    for (let i = 0; i < 10; i += 1) {
      const step = wheelVolumeDelta(-6, carry);
      carry = step.carry;
      total += step.delta;
    }
    expect(total).toBe(3);
    expect(carry).toBe(0);
  });

  it('does nothing for no movement', () => {
    expect(wheelVolumeDelta(0, 0)).toEqual({ delta: 0, carry: 0 });
  });
});
