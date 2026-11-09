/** Where the check frame is on the page: left, top, width, height in CSS pixels. */
export type PanelRect = [number, number, number, number];

/** The part of a DOM rectangle that matters (what `getBoundingClientRect` returns). */
export interface BoxLike {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** The frame's place, rounded to whole pixels so tiny layout jitter does not count as a move. */
export function rectOf(box: BoxLike): PanelRect {
  return [Math.round(box.left), Math.round(box.top), Math.round(box.width), Math.round(box.height)];
}

/** Whether two places are the same. */
export function sameRect(a: PanelRect | null, b: PanelRect | null): boolean {
  if (a === null || b === null) return a === b;
  return a.every((value, index) => value === b[index]);
}

/** Whether the frame can hold a view: the backend refuses empty or absurd places, so ask first. */
export function isUsableRect(rect: PanelRect): boolean {
  const [x, y, width, height] = rect;
  return [x, y, width, height].every(Number.isFinite) && width >= 1 && height >= 1;
}
