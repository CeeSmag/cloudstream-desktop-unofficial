import React from 'react';
import { api } from '../services/api';
import { DebugLoggingInfo } from '../types';
import { setDebugLogging } from '../utils/logger';

/** How often the file sizes are refreshed while debug logging is on and the page is open. */
const REFRESH_MS = 3000;

/** The debug logging switch and the files it writes, for the General settings page. */
export function useDebugLogging(report: (error: unknown) => void) {
  const [info, setInfo] = React.useState<DebugLoggingInfo | null>(null);
  const [busy, setBusy] = React.useState(false);

  const apply = React.useCallback((latest: DebugLoggingInfo) => {
    setInfo(latest);
    setDebugLogging(latest.enabled);
  }, []);

  const refresh = React.useCallback(
    () => api.getDebugLogging().then(apply).catch(report),
    [apply, report],
  );

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  // Sizes only change while logging is on, so nothing is polled otherwise.
  const enabled = info?.enabled ?? false;
  React.useEffect(() => {
    if (!enabled) return undefined;
    const timer = setInterval(() => void refresh(), REFRESH_MS);
    return () => clearInterval(timer);
  }, [enabled, refresh]);

  const toggle = async (on: boolean) => {
    setBusy(true);
    try {
      apply(await api.setDebugLogging(on));
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  };

  const openFolder = () => api.openLogsFolder().catch(report);

  return { info, busy, toggle, openFolder };
}
