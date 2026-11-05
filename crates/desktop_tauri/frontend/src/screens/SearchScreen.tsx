import React from 'react';
import { Search as SearchIcon, X, Film } from 'lucide-react';
import { HomeItem, SearchItem } from '../types';
import { PosterCard } from '../components/PosterCard';

interface SearchScreenProps {
  onSelectItem: (item: HomeItem) => void;
  selectedProvider: string;
  /** The search text lives in the app so it survives opening a title and coming back. */
  query: string;
  onQueryChange: (query: string) => void;
  onSearch: (query: string) => Promise<SearchItem[]>;
}

export const SearchScreen: React.FC<SearchScreenProps> = ({
  onSelectItem,
  selectedProvider,
  query,
  onQueryChange,
  onSearch,
}) => {
  const [results, setResults] = React.useState<SearchItem[]>([]);
  const [loading, setLoading] = React.useState(false);

  React.useEffect(() => {
    if (!query.trim()) {
      setResults([]);
      return;
    }
    const timer = setTimeout(async () => {
      setLoading(true);
      try {
        const res = await onSearch(query);
        setResults(res);
      } finally {
        setLoading(false);
      }
    }, 300);

    return () => clearTimeout(timer);
  }, [query, onSearch]);

  return (
    <div className="flex-1 overflow-y-auto page-gutter py-6 sm:py-8 max-w-[100rem] mx-auto w-full select-none">
      {/* Search Header Bar */}
      <div className="relative max-w-2xl mx-auto mb-6 sm:mb-10">
        <div className="relative flex items-center">
          <SearchIcon className="absolute left-4 w-5 h-5 text-zinc-400" />
          <input
            type="text"
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder={`Search titles across ${selectedProvider || 'all providers'}...`}
            autoFocus
            className="w-full pl-12 pr-12 py-3.5 rounded-2xl bg-surface-elevated/95 border border-white/10 focus:border-accent text-white placeholder-zinc-500 text-sm focus:outline-none focus:ring-2 focus:ring-accent/20 transition-colors shadow-xl"
          />
          {query && (
            <button
              onClick={() => onQueryChange('')}
              className="absolute right-4 p-1 rounded-full text-zinc-400 hover:text-white transition-colors"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>
      </div>

      {/* Results Content */}
      {loading ? (
        <div className="flex flex-col items-center justify-center py-20 text-zinc-500">
          <div className="w-8 h-8 rounded-full border-2 border-accent border-t-transparent animate-spin mb-4" />
          <span className="text-sm">Searching providers...</span>
        </div>
      ) : results.length > 0 ? (
        <div>
          <h2 className="text-sm font-semibold text-zinc-400 uppercase tracking-wider mb-6">
            Found {results.length} Results
          </h2>
          <div className="poster-grid">
            {results.map((item) => (
              <PosterCard
                key={item.id}
                item={{
                  id: item.id,
                  name: item.name,
                  url: item.url,
                  poster_url: item.poster_url,
                  provider: item.provider,
                  media_type: null,
                  year: item.year,
                }}
                onClick={onSelectItem}
              />
            ))}
          </div>
        </div>
      ) : query.trim() ? (
        <div className="flex flex-col items-center justify-center py-24 text-zinc-500">
          <Film className="w-12 h-12 stroke-[1.5] mb-3 text-zinc-600" />
          <p className="text-base font-medium text-zinc-400">No titles found</p>
          <p className="text-xs text-zinc-600 mt-1">Try another keyword or switch providers</p>
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center py-24 text-zinc-500">
          <SearchIcon className="w-12 h-12 stroke-[1.5] mb-3 text-zinc-600" />
          <p className="text-base font-medium text-zinc-400">Search for movies, series & anime</p>
          <p className="text-xs text-zinc-600 mt-1">Results will appear as you type</p>
        </div>
      )}
    </div>
  );
};
