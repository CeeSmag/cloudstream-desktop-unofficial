import React from 'react';
import { ClickSorter, createClickSorter } from '../utils/clickSorter';

/**
 * Separates a single click from a double click. The single action runs at once (no wait to see
 * whether a second click follows); a double click takes it back and runs the double action, so it
 * does not leave the video paused. See [`createClickSorter`].
 */
export function useClickOrDoubleClick(onSingle: () => void, onDouble: () => void): ClickSorter {
  const latest = React.useRef({ onSingle, onDouble });
  latest.current = { onSingle, onDouble };
  const [sorter] = React.useState(() =>
    createClickSorter(
      () => latest.current.onSingle(),
      () => latest.current.onDouble(),
    ),
  );
  return sorter;
}
