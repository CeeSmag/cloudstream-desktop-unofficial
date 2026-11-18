import React from 'react';
import { isDebugLogging, logEvent } from '../utils/logger';

/** How often the page checks that it is still responsive. */
const CHECK_EVERY_MS = 500;
/** A check that comes this much later than planned is reported as a stall. */
const STALL_REPORT_MS = 400;

/**
 * While debug logging is on, reports every time the page stalled (its main thread was busy and
 * could not react to the mouse), with the state it was in, so a freeze leaves a trace in the logs.
 */
export function useStallWatch(context: () => string): void {
  const describe = React.useRef(context);
  describe.current = context;

  React.useEffect(() => {
    if (!isDebugLogging()) return undefined;
    let expected = performance.now() + CHECK_EVERY_MS;
    const timer = setInterval(() => {
      const late = performance.now() - expected;
      if (late > STALL_REPORT_MS) {
        logEvent('warn', `page stalled for ${Math.round(late)} ms (${describe.current()})`);
      }
      expected = performance.now() + CHECK_EVERY_MS;
    }, CHECK_EVERY_MS);
    return () => clearInterval(timer);
  }, []);
}
