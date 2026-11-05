import React from 'react';
import { Star, Film, Tv, Flame, SlidersHorizontal, Calendar } from 'lucide-react';
import { BrowseFilters } from '../types';
import {
  CURRENT_YEAR,
  GENRES,
  RATINGS,
  YEARS,
  selectSort,
  toggleGenre,
} from '../utils/browseFilters';

interface DragScrollRowProps {
  children: React.ReactNode;
}

const DragScrollRow: React.FC<DragScrollRowProps> = ({ children }) => {
  const rowRef = React.useRef<HTMLDivElement>(null);
  const drag = React.useRef({ isDown: false, startX: 0, scrollLeft: 0, moved: false });

  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    const el = rowRef.current;
    if (!el) return;
    drag.current = { isDown: true, startX: e.pageX, scrollLeft: el.scrollLeft, moved: false };
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    const el = rowRef.current;
    if (!el || !drag.current.isDown) return;
    const dx = e.pageX - drag.current.startX;
    if (Math.abs(dx) > 4) {
      drag.current.moved = true;
      el.scrollLeft = drag.current.scrollLeft - dx;
    }
  };

  const stopDrag = () => {
    drag.current.isDown = false;
  };

  const handleClickCapture = (e: React.MouseEvent<HTMLDivElement>) => {
    if (drag.current.moved) {
      e.stopPropagation();
      e.preventDefault();
      drag.current.moved = false;
    }
  };

  const handleWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    const el = rowRef.current;
    if (el && e.deltaY !== 0) {
      el.scrollLeft += e.deltaY;
    }
  };

  return (
    <div
      ref={rowRef}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={stopDrag}
      onMouseLeave={stopDrag}
      onClickCapture={handleClickCapture}
      onWheel={handleWheel}
      className="flex items-center gap-1.5 overflow-x-auto no-scrollbar py-1 cursor-grab active:cursor-grabbing select-none"
    >
      {children}
    </div>
  );
};

interface BrowseFilterBarProps {
  filters: BrowseFilters;
  onUpdateFilters: (updater: (prev: BrowseFilters) => BrowseFilters) => void;
}

export const BrowseFilterBar: React.FC<BrowseFilterBarProps> = ({ filters, onUpdateFilters }) => {
  const lastRatingRef = React.useRef<number>(filters.rating ?? 7);

  React.useEffect(() => {
    if (filters.rating !== null) {
      lastRatingRef.current = filters.rating;
    }
  }, [filters.rating]);

  const handleSortChange = (sort: 'popular' | 'rating') => {
    onUpdateFilters((prev) => {
      const preferredRating = prev.rating ?? lastRatingRef.current;
      const { sort_by, rating } = selectSort(sort, preferredRating);
      return { ...prev, sort_by, rating, page: 1 };
    });
  };

  const handleRatingChange = (rating: number) => {
    lastRatingRef.current = rating;
    onUpdateFilters((prev) => ({ ...prev, sort_by: 'rating', rating, page: 1 }));
  };

  return (
    <div className="flex flex-col gap-4 mb-6 bg-surface-elevated/90 p-4 sm:p-5 rounded-2xl border border-white/10 shadow-lg">
      <PrimaryTabsRow
        filters={filters}
        onMediaType={(media_type) => onUpdateFilters((prev) => ({ ...prev, media_type, page: 1 }))}
        onYear={(year) => onUpdateFilters((prev) => ({ ...prev, year, page: 1 }))}
        onSort={handleSortChange}
      />

      {filters.sort_by === 'rating' && (
        <DragScrollRow>
          <span className="text-xs font-medium text-zinc-400 flex items-center gap-1 mr-1 whitespace-nowrap">
            <SlidersHorizontal className="w-3.5 h-3.5" />
            <span>Rating (0–10):</span>
          </span>
          {RATINGS.map((r) => (
            <button
              key={r}
              onClick={() => handleRatingChange(r)}
              className={`px-2.5 py-1 rounded-lg text-xs font-semibold border transition-colors whitespace-nowrap shrink-0 ${
                (filters.rating ?? 0) === r
                  ? 'bg-accent/20 border-accent text-accent font-bold'
                  : 'bg-white/5 border-white/5 text-zinc-400 hover:bg-white/10'
              }`}
            >
              {r === 0 ? '0 (All)' : r === 10 ? '10 ★' : `${r}+ ★`}
            </button>
          ))}
        </DragScrollRow>
      )}

      <DragScrollRow>
        <button
          onClick={() => onUpdateFilters((prev) => ({ ...prev, genres: [], page: 1 }))}
          className={`px-3 py-1 rounded-lg text-xs font-medium border transition-colors whitespace-nowrap shrink-0 ${
            filters.genres.length === 0
              ? 'bg-accent text-white border-accent shadow-sm font-semibold'
              : 'bg-white/5 border-white/5 text-zinc-400 hover:bg-white/10 hover:text-white'
          }`}
        >
          All Genres
        </button>
        {GENRES.map((g) => {
          const isSelected = filters.genres.includes(g);
          return (
            <button
              key={g}
              onClick={() =>
                onUpdateFilters((prev) => ({
                  ...prev,
                  genres: toggleGenre(prev.genres, g),
                  page: 1,
                }))
              }
              className={`px-3 py-1 rounded-lg text-xs font-medium border transition-colors whitespace-nowrap shrink-0 ${
                isSelected
                  ? 'bg-accent text-white border-accent shadow-sm font-semibold'
                  : 'bg-white/5 border-white/5 text-zinc-400 hover:bg-white/10 hover:text-white'
              }`}
            >
              {g}
            </button>
          );
        })}
      </DragScrollRow>
    </div>
  );
};

interface PrimaryTabsRowProps {
  filters: BrowseFilters;
  onMediaType: (type: 'movie' | 'tv') => void;
  onYear: (year: number | null) => void;
  onSort: (sort: 'popular' | 'rating') => void;
}

const PrimaryTabsRow: React.FC<PrimaryTabsRowProps> = ({
  filters,
  onMediaType,
  onYear,
  onSort,
}) => (
  <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/5 pb-4">
    <div className="flex flex-wrap items-center gap-2 sm:gap-3">
      <div className="flex items-center bg-black/40 p-1 rounded-xl border border-white/10">
        <button
          onClick={() => onMediaType('movie')}
          className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors ${
            filters.media_type === 'movie'
              ? 'bg-accent text-white shadow-md'
              : 'text-zinc-400 hover:text-white'
          }`}
        >
          <Film className="w-3.5 h-3.5" />
          <span>Movies</span>
        </button>
        <button
          onClick={() => onMediaType('tv')}
          className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors ${
            filters.media_type === 'tv'
              ? 'bg-accent text-white shadow-md'
              : 'text-zinc-400 hover:text-white'
          }`}
        >
          <Tv className="w-3.5 h-3.5" />
          <span>TV Series</span>
        </button>
      </div>

      <div className="flex items-center gap-1.5 bg-black/40 px-3 py-1.5 rounded-xl border border-white/10 text-xs">
        <Calendar className="w-3.5 h-3.5 text-zinc-400" />
        <span className="text-zinc-400 font-medium hidden sm:inline">Year:</span>
        <select
          value={filters.year ?? ''}
          onChange={(e) => onYear(e.target.value ? Number(e.target.value) : null)}
          className="bg-transparent text-white font-semibold text-xs outline-none cursor-pointer pr-1"
        >
          <option value="" className="bg-[#161624] text-zinc-300">All Years</option>
          {YEARS.map((y) => (
            <option key={y} value={y} className="bg-[#161624] text-white">
              {y} {y === CURRENT_YEAR ? '(Latest)' : ''}
            </option>
          ))}
        </select>
      </div>
    </div>

    <div className="flex items-center bg-black/40 p-1 rounded-xl border border-white/10">
      <button
        onClick={() => onSort('popular')}
        className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors ${
          filters.sort_by === 'popular'
            ? 'bg-accent text-white shadow-md'
            : 'text-zinc-400 hover:text-white'
        }`}
      >
        <Flame className="w-3.5 h-3.5" />
        <span>Popular</span>
      </button>
      <button
        onClick={() => onSort('rating')}
        className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors ${
          filters.sort_by === 'rating'
            ? 'bg-accent text-white shadow-md'
            : 'text-zinc-400 hover:text-white'
        }`}
      >
        <Star className="w-3.5 h-3.5" />
        <span>Rating</span>
      </button>
    </div>
  </div>
);
