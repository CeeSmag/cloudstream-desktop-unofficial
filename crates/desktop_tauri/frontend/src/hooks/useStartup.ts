import React from 'react';
import { api } from '../services/api';
import { StartupStatus } from '../types';
import { onBackendEvent } from '../utils/backendEvents';

/** Shown until the backend reports its first status. */
const STARTING: StartupStatus = { ready: false, finished: 0, total: 0, current: '', failures: [] };

/** How far the app's start-up (loading the installed extensions) has come. */
export function useStartup(): StartupStatus {
  const [status, setStatus] = React.useState<StartupStatus>(STARTING);

  React.useEffect(() => {
    const stop = onBackendEvent<StartupStatus>('startup-progress', setStatus);
    // Start-up may have progressed before this screen began listening.
    api
      .getStartupStatus()
      .then((latest) => setStatus((current) => (current.ready ? current : latest)))
      .catch(() => setStatus({ ...STARTING, ready: true }));
    return stop;
  }, []);

  return status;
}
