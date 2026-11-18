/** Tells a single click on the video from a double click without making the single click wait. */
export interface ClickSorter {
  /**
   * A click on the video. `clicks` is the browser's count for it (`MouseEvent.detail`): 1 for a
   * first click, 2 for the second click of a double click, 0 for a click that did something else
   * (it closed a menu).
   */
  onClick: (clicks: number) => void;
  onDoubleClick: () => void;
}

/**
 * The single action (pause or play) runs at once on the first click. A double click is a first
 * click, a second click and then the double click itself: the second click does nothing, and the
 * double click takes the first click's action back before it runs its own (fullscreen), so the
 * video ends up playing or paused as it was.
 */
export function createClickSorter(onSingle: () => void, onDouble: () => void): ClickSorter {
  /** Whether the last first click ran the single action, so a double click knows to take it back. */
  let ranSingle = false;
  return {
    onClick: (clicks) => {
      if (clicks > 1) return;
      ranSingle = clicks === 1;
      if (ranSingle) onSingle();
    },
    onDoubleClick: () => {
      if (ranSingle) onSingle();
      ranSingle = false;
      onDouble();
    },
  };
}
