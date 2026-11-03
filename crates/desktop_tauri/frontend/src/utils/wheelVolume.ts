/** Wheel distance that changes the volume by one percent; one notch of a mouse wheel (100) is 5%. */
export const WHEEL_UNITS_PER_PERCENT = 20;

/**
 * Turns a wheel movement into a volume change in percent. Scrolling up raises the volume, down
 * lowers it. `carry` is the part of earlier movements that did not add up to a whole percent yet,
 * so the small steps of a touchpad are not lost; the new carry is returned with the change.
 */
export function wheelVolumeDelta(deltaY: number, carry: number): { delta: number; carry: number } {
  const total = carry - deltaY;
  const delta = Math.trunc(total / WHEEL_UNITS_PER_PERCENT);
  return { delta, carry: total - delta * WHEEL_UNITS_PER_PERCENT };
}
