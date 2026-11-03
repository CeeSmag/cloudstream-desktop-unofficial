import React from 'react';
import { api } from '../services/api';
import { InstalledExtension, RepositoryEntry, RepositoryPlugin } from '../types';
import { onBackendEvent } from '../utils/backendEvents';

/** State and actions of the Extensions page: installed extensions, repositories, and one repository's plugins. */
export function useExtensions(report: (error: unknown) => void) {
  const [installed, setInstalled] = React.useState<InstalledExtension[]>([]);
  const [repositories, setRepositories] = React.useState<RepositoryEntry[]>([]);
  const [browsing, setBrowsing] = React.useState<string | null>(null);
  const [plugins, setPlugins] = React.useState<RepositoryPlugin[]>([]);
  const [pluginsLoading, setPluginsLoading] = React.useState(false);
  /** Key of the action that is running (`install:Name`, `add-repository`, ...), or null. */
  const [busy, setBusy] = React.useState<string | null>(null);

  const refreshInstalled = React.useCallback(
    () => api.getInstalledExtensions().then(setInstalled).catch(report),
    [report],
  );
  const refreshRepositories = React.useCallback(
    () => api.getRepositories().then(setRepositories).catch(report),
    [report],
  );

  React.useEffect(() => {
    void refreshInstalled();
    void refreshRepositories();
    const stopProviders = onBackendEvent('providers-changed', () => void refreshInstalled());
    const stopRepositories = onBackendEvent('repositories-changed', () => void refreshRepositories());
    return () => {
      stopProviders();
      stopRepositories();
    };
  }, [refreshInstalled, refreshRepositories]);

  const loadPlugins = React.useCallback(
    async (url: string) => {
      setPluginsLoading(true);
      try {
        setPlugins(await api.getRepositoryPlugins(url));
      } catch (error) {
        setPlugins([]);
        report(error);
      } finally {
        setPluginsLoading(false);
      }
    },
    [report],
  );

  const browse = (url: string) => {
    setBrowsing(url);
    void loadPlugins(url);
  };

  /** Runs one action with a busy marker; returns whether it succeeded. */
  const run = async (key: string, action: () => Promise<void>): Promise<boolean> => {
    setBusy(key);
    try {
      await action();
      return true;
    } catch (error) {
      report(error);
      return false;
    } finally {
      setBusy(null);
    }
  };

  const addRepository = (url: string) =>
    run('add-repository', async () => {
      await api.addRepository(url);
      await refreshRepositories();
    });

  const removeRepository = (url: string) =>
    run(`remove-repository:${url}`, async () => {
      await api.removeRepository(url);
      if (browsing === url) {
        setBrowsing(null);
        setPlugins([]);
      }
      await refreshRepositories();
    });

  const install = (repositoryUrl: string, internalName: string) =>
    run(`install:${internalName}`, async () => {
      await api.installPlugin(repositoryUrl, internalName);
      await refreshInstalled();
      await loadPlugins(repositoryUrl);
    });

  const uninstall = (internalName: string) =>
    run(`uninstall:${internalName}`, async () => {
      await api.uninstallExtension(internalName);
      await refreshInstalled();
      if (browsing) await loadPlugins(browsing);
    });

  const setTrusted = (internalName: string, trusted: boolean) =>
    run(`trust:${internalName}`, async () => {
      await api.setExtensionTrusted(internalName, trusted);
      await refreshInstalled();
    });

  return {
    installed,
    repositories,
    browsing,
    plugins,
    pluginsLoading,
    busy,
    browse,
    addRepository,
    removeRepository,
    install,
    uninstall,
    setTrusted,
  };
}
