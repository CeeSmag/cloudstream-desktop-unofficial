import React from 'react';

/** How long a click waits to see whether it is the first half of a double click. */
export const DOUBLE_CLICK_MS = 250;

/**
 * Separates a single click from a double click: the single action runs only if no second click
 * follows within [`DOUBLE_CLICK_MS`], so a double click does not also pause and resume the video.
 */
export function useClickOrDoubleClick(onSingle: () => void, onDouble: () => void) {
  const timer = React.useRef<number | null>(null);
  const latest = React.useRef({ onSingle, onDouble });
  latest.current = { onSingle, onDouble };

  const cancel = React.useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
  }, []);

  React.useEffect(() => cancel, [cancel]);

  const onClick = () => {
    cancel();
    timer.current = window.setTimeout(() => {
      timer.current = null;
      latest.current.onSingle();
    }, DOUBLE_CLICK_MS);
  };

  const onDoubleClick = () => {
    cancel();
    latest.current.onDouble();
  };

  return { onClick, onDoubleClick };
}
