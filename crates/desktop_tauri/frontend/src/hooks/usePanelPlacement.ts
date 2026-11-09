import React from 'react';
import { api } from '../services/api';
import { describeValue, logEvent } from '../utils/logger';
import { isUsableRect, PanelRect, rectOf, sameRect } from '../utils/panelRect';

/** How often the frame is measured again, for layout changes no event announces. */
const REMEASURE_MS = 1000;
/** How often an open check says so in the log, so a stuck check can be told from a slow one. */
const HEARTBEAT_MS = 5000;

/**
 * Keeps the check view (a native view the backend draws on top of the page) over `frame` while a check is
 * open: tells the backend the frame's place at first and whenever it changes. Every failure is logged and
 * handed to `onProblem`; nothing is swallowed.
 */
export function usePanelPlacement(
  host: string | null,
  frame: React.RefObject<HTMLElement>,
  onProblem: (reason: string | null) => void,
): void {
  React.useEffect(() => {
    const element = frame.current;
    if (!host || !element) return undefined;
    let last: PanelRect | null = null;
    let sent = 0;
    let stopped = false;

    const place = () => {
      if (stopped) return;
      const rect = rectOf(element.getBoundingClientRect());
      if (sameRect(rect, last)) return;
      if (!isUsableRect(rect)) {
        logEvent('warn', `check frame for ${host} has no usable place yet: ${JSON.stringify(rect)}`);
        return;
      }
      last = rect;
      sent += 1;
      logEvent('info', `check frame for ${host} placed at ${JSON.stringify(rect)} (send #${sent})`);
      api
        .placeCloudflarePanel(host, rect)
        .then(() => onProblem(null))
        .catch((error: unknown) => {
          last = null; // try again at the next measurement
          const reason = describeValue(error);
          logEvent('error', `the check view could not be placed for ${host}: ${reason}`);
          onProblem(reason);
        });
    };

    place();
    const observer = new ResizeObserver(place);
    observer.observe(element);
    window.addEventListener('resize', place);
    const remeasure = window.setInterval(place, REMEASURE_MS);
    const heartbeat = window.setInterval(() => {
      logEvent('info', `check frame for ${host} is open; last place ${JSON.stringify(last)}, ${sent} sends`);
    }, HEARTBEAT_MS);
    return () => {
      stopped = true;
      observer.disconnect();
      window.removeEventListener('resize', place);
      window.clearInterval(remeasure);
      window.clearInterval(heartbeat);
      logEvent('info', `check frame for ${host} closed after ${sent} placements`);
    };
  }, [host, frame, onProblem]);
}
