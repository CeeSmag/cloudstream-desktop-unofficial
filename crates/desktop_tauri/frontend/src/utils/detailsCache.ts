/** A small memory of loaded values, oldest forgotten first once `limit` is reached. */
export interface MemoryCache<V> {
  get: (key: string) => V | undefined;
  set: (key: string, value: V) => void;
  size: () => number;
}

export function createMemoryCache<V>(limit: number): MemoryCache<V> {
  const entries = new Map<string, V>();
  return {
    get: (key) => entries.get(key),
    set: (key, value) => {
      entries.delete(key); // a re-saved key counts as the newest
      entries.set(key, value);
      while (entries.size > limit) {
        const oldest = entries.keys().next();
        if (oldest.done) break;
        entries.delete(oldest.value);
      }
    },
    size: () => entries.size,
  };
}
