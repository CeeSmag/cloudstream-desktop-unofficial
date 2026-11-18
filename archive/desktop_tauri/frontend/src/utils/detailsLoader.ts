/** How the loader remembers titles. */
export interface DetailsLoaderOptions {
  /** How long a loaded title is reused. */
  ttlMs: number;
  /** How many titles are remembered (oldest forgotten first). */
  limit: number;
  now: () => number;
  /** Says what the loader did, for the log. */
  note: (message: string) => void;
}

interface Remembered<V> {
  value: V;
  at: number;
}

/**
 * Loads titles through `fetcher` with two savings: a title loaded a short while ago is returned at once, and
 * asking for a title that is already loading joins that load instead of queuing another (a long show can take
 * a minute and a half, and the extension runtime answers one call at a time, so each extra load would delay
 * everything behind it). A failed load is never remembered.
 */
export function createDetailsLoader<V>(
  fetcher: (provider: string, url: string) => Promise<V>,
  options: DetailsLoaderOptions,
): (provider: string, url: string) => Promise<V> {
  const remembered = new Map<string, Remembered<V>>();
  const loading = new Map<string, Promise<V>>();

  const remember = (key: string, value: V) => {
    remembered.delete(key);
    remembered.set(key, { value, at: options.now() });
    while (remembered.size > options.limit) {
      const oldest = remembered.keys().next();
      if (oldest.done) break;
      remembered.delete(oldest.value);
    }
  };

  return (provider, url) => {
    const key = `${provider}\n${url}`;
    const saved = remembered.get(key);
    if (saved && options.now() - saved.at < options.ttlMs) {
      options.note(`details of ${provider} title answered from memory`);
      return Promise.resolve(saved.value);
    }
    const running = loading.get(key);
    if (running) {
      options.note(`details of ${provider} title are already loading; joining that load`);
      return running;
    }
    const load = fetcher(provider, url)
      .then((value) => {
        remember(key, value);
        return value;
      })
      .finally(() => {
        loading.delete(key);
      });
    loading.set(key, load);
    return load;
  };
}
