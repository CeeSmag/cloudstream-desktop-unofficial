import React from 'react';
import { api } from '../services/api';
import { PluginHit } from '../types';

/**
 * The extensions of every repository, for the Plugins tab. The first load downloads all the lists and
 * can take several seconds; `reload` fetches them again (for example after an install) without
 * showing the loading state again.
 */
export function useAllPlugins(report: (error: unknown) => void) {
  const [hits, setHits] = React.useState<PluginHit[]>([]);
  const [loading, setLoading] = React.useState(true);

  const reload = React.useCallback(async () => {
    try {
      setHits(await api.listAllPlugins());
    } catch (error) {
      report(error);
    } finally {
      setLoading(false);
    }
  }, [report]);

  React.useEffect(() => {
    void reload();
  }, [reload]);

  return { hits, loading, reload };
}
