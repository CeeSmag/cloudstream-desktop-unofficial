import React from 'react';
import { ChevronLeft, ChevronRight, Film } from 'lucide-react';
import { BrowseFilters, BrowseItem, BrowsePageResult } from '../types';
import { api, describeError } from '../services/api';
import { BrowseCard } from '../components/BrowseCard';
import { BrowseFilterBar } from '../components/BrowseFilterBar';
import { defaultBrowseFilters, serializeFilterKey } from '../utils/browseFilters';

interface BrowseScreenProps {
  filters: BrowseFilters;
  selectedProvider: string;
  onUpdateFilters: (updater: (prev: BrowseFilters) => BrowseFilters) => void;
  onSelectItem: (item: BrowseItem) => void;
  onError: (error: string) => void;
  cachedData?: BrowsePageResult | null;
  cachedFilterKey?: string;
  onCacheData?: (key: string, data: BrowsePageResult) => void;
}

export const BrowseScreen: React.FC<BrowseScreenProps> = ({
  filters,
  selectedProvider,
  onUpdateFilters,
  onSelectItem,
  onError,
  cachedData = null,
  cachedFilterKey = '',
  onCacheData,
}) => {
  const filterKey = serializeFilterKey(filters, selectedProvider);
  const hasCachedMatch = cachedData !== null && cachedFilterKey === filterKey;
  const [localData, setLocalData] = React.useState<BrowsePageResult | null>(cachedData);
  const [loading, setLoading] = React.useState(!hasCachedMatch);
  const scrollContainerRef = React.useRef<HTMLDivElement>(null);
  const requestIdRef = React.useRef(0);

  const data = hasCachedMatch ? cachedData : localData;

  React.useEffect(() => {
    if (hasCachedMatch) {
      setLocalData(cachedData);
      setLoading(false);
      return;
    }
    let cancelled = false;
    const reqId = ++requestIdRef.current;
    setLoading(true);

    api
      .browseCatalog(filters)
      .then((res) => {
        if (cancelled || reqId !== requestIdRef.current) return;
        setLocalData(res);
        onCacheData?.(filterKey, res);
      })
      .catch((err) => {
        if (cancelled || reqId !== requestIdRef.current) return;
        onError(describeError(err));
      })
      .finally(() => {
        if (!cancelled && reqId === requestIdRef.current) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [filters, filterKey, hasCachedMatch, cachedData, onCacheData, onError]);

  const handlePageChange = (newPage: number) => {
    if (newPage < 1 || (data && newPage > data.total_pages)) return;
    onUpdateFilters((prev) => ({ ...prev, page: newPage }));
    scrollContainerRef.current?.scrollTo({ top: 0, behavior: 'smooth' });
  };

  return (
    <div
      ref={scrollContainerRef}
      className="flex-1 flex flex-col overflow-y-auto page-gutter py-6 select-none max-w-[105rem] mx-auto w-full"
    >
      <BrowseFilterBar filters={filters} onUpdateFilters={onUpdateFilters} />

      {loading ? (
        <div className="flex-1 flex flex-col items-center justify-center py-24 text-zinc-500">
          <div className="w-10 h-10 rounded-full border-2 border-accent border-t-transparent animate-spin mb-4" />
          <span className="text-sm">Loading browse titles...</span>
        </div>
      ) : data && data.items.length > 0 ? (
        <>
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-3.5 sm:gap-4 lg:gap-5">
            {data.items.map((item) => (
              <BrowseCard key={item.id} item={item} onClick={() => onSelectItem(item)} />
            ))}
          </div>

          <PaginationBar
            page={filters.page}
            totalPages={data.total_pages}
            onPageChange={handlePageChange}
          />
        </>
      ) : (
        <div className="flex-1 flex flex-col items-center justify-center py-24 text-zinc-500">
          <Film className="w-12 h-12 stroke-[1.5] mb-3 text-zinc-600" />
          <p className="text-base font-medium text-zinc-400">No titles match these filters</p>
          <button
            onClick={() => onUpdateFilters(() => defaultBrowseFilters)}
            className="mt-4 px-4 py-2 rounded-xl bg-accent text-white text-xs font-semibold hover:bg-accent-hover transition-colors"
          >
            Reset Filters
          </button>
        </div>
      )}
    </div>
  );
};

interface PaginationBarProps {
  page: number;
  totalPages: number;
  onPageChange: (page: number) => void;
}

const PaginationBar: React.FC<PaginationBarProps> = ({ page, totalPages, onPageChange }) => (
  <div className="mt-8 mb-4 flex items-center justify-center gap-2 sm:gap-3 py-4 border-t border-white/5">
    <button
      onClick={() => onPageChange(page - 1)}
      disabled={page <= 1}
      className="flex items-center gap-1 px-3.5 py-2 rounded-xl text-xs font-medium bg-white/5 hover:bg-white/10 disabled:opacity-30 disabled:cursor-not-allowed border border-white/10 transition-colors"
    >
      <ChevronLeft className="w-4 h-4" />
      <span>Previous</span>
    </button>

    <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-black/30 border border-white/10 text-xs font-semibold text-zinc-300">
      <span className="text-accent">{page}</span>
      <span className="text-zinc-500">/</span>
      <span>{totalPages}</span>
    </div>

    <button
      onClick={() => onPageChange(page + 1)}
      disabled={page >= totalPages}
      className="flex items-center gap-1 px-3.5 py-2 rounded-xl text-xs font-medium bg-white/5 hover:bg-white/10 disabled:opacity-30 disabled:cursor-not-allowed border border-white/10 transition-colors"
    >
      <span>Next</span>
      <ChevronRight className="w-4 h-4" />
    </button>
  </div>
);
