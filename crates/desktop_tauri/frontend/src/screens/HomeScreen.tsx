import React from 'react';
import { Play, Info, BookmarkPlus, ChevronRight, ChevronLeft } from 'lucide-react';
import { HomeItem, HomeSection } from '../types';
import { PosterCard } from '../components/PosterCard';

interface HomeScreenProps {
  sections: HomeSection[];
  onSelectItem: (item: HomeItem) => void;
  onPlayItem: (item: HomeItem) => void;
  loading: boolean;
}

export const HomeScreen: React.FC<HomeScreenProps> = ({
  sections,
  onSelectItem,
  onPlayItem,
  loading,
}) => {
  const heroItem = sections[0]?.items[0];

  return (
    <div className="flex-1 overflow-y-auto pb-20 select-none">
      {/* Hero Banner */}
      {heroItem && (
        <div className="relative w-full h-[460px] overflow-hidden">
          {/* Backdrop Image */}
          <div className="absolute inset-0">
            {heroItem.poster_url && (
              <img
                src={heroItem.poster_url}
                alt={heroItem.name}
                className="w-full h-full object-cover object-top blur-[2px] opacity-40 scale-105"
              />
            )}
            {/* Gradient Overlays */}
            <div className="absolute inset-0 bg-gradient-to-t from-midnight via-midnight/60 to-transparent" />
            <div className="absolute inset-0 bg-gradient-to-r from-midnight via-midnight/80 to-transparent" />
          </div>

          {/* Hero Content */}
          <div className="relative h-full max-w-7xl mx-auto px-8 flex flex-col justify-end pb-12 z-10">
            <div className="flex items-center space-x-2 mb-3">
              <span className="px-2.5 py-0.5 rounded-md bg-accent/20 text-accent font-semibold text-xs border border-accent/30 tracking-wide uppercase">
                Featured
              </span>
              {heroItem.media_type && (
                <span className="px-2.5 py-0.5 rounded-md bg-white/10 text-zinc-300 font-medium text-xs border border-white/10">
                  {heroItem.media_type}
                </span>
              )}
              {heroItem.year && (
                <span className="text-zinc-400 text-xs font-medium">
                  {heroItem.year}
                </span>
              )}
            </div>

            <h1 className="text-4xl md:text-5xl font-extrabold text-white tracking-tight drop-shadow-md mb-3 max-w-2xl">
              {heroItem.name}
            </h1>

            {/* Action Buttons */}
            <div className="flex items-center space-x-3.5">
              <button
                onClick={() => onPlayItem(heroItem)}
                className="flex items-center space-x-2.5 px-6 py-3 rounded-xl bg-accent hover:bg-accent-hover text-white text-sm font-semibold transition-all duration-200 shadow-lg glow-accent"
              >
                <Play className="w-4 h-4 fill-current" />
                <span>Play Now</span>
              </button>

              <button
                onClick={() => onSelectItem(heroItem)}
                className="flex items-center space-x-2 px-5 py-3 rounded-xl bg-white/10 hover:bg-white/15 text-white text-sm font-medium border border-white/10 transition-colors backdrop-blur-md"
              >
                <Info className="w-4 h-4" />
                <span>Details</span>
              </button>

              <button
                className="p-3 rounded-xl bg-white/10 hover:bg-white/15 text-zinc-300 hover:text-white border border-white/10 transition-colors backdrop-blur-md"
                title="Add to Library"
              >
                <BookmarkPlus className="w-4 h-4" />
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Sections / Shelves */}
      <div className="max-w-7xl mx-auto px-8 space-y-10 mt-4">
        {loading && sections.length === 0 ? (
          <div className="flex items-center justify-center py-24 text-zinc-500">
            <div className="w-8 h-8 rounded-full border-2 border-accent border-t-transparent animate-spin mb-4" />
            <span className="ml-3 text-sm">Loading feed...</span>
          </div>
        ) : (
          sections.map((section, idx) => (
            <ShelfRow
              key={`${section.title}-${idx}`}
              section={section}
              onSelectItem={onSelectItem}
              onPlayItem={onPlayItem}
            />
          ))
        )}
      </div>
    </div>
  );
};

const ShelfRow: React.FC<{
  section: HomeSection;
  onSelectItem: (item: HomeItem) => void;
  onPlayItem: (item: HomeItem) => void;
}> = ({ section, onSelectItem, onPlayItem }) => {
  const rowRef = React.useRef<HTMLDivElement>(null);

  const scroll = (direction: 'left' | 'right') => {
    if (rowRef.current) {
      const offset = direction === 'left' ? -500 : 500;
      rowRef.current.scrollBy({ left: offset, behavior: 'smooth' });
    }
  };

  return (
    <div className="relative group/shelf">
      {/* Section Title Header */}
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-xl font-bold text-white tracking-tight flex items-center gap-2">
          {section.title}
          <ChevronRight className="w-4 h-4 text-zinc-500 group-hover/shelf:text-accent transition-colors" />
        </h2>
        <div className="flex items-center space-x-1.5 opacity-0 group-hover/shelf:opacity-100 transition-opacity">
          <button
            onClick={() => scroll('left')}
            className="p-1.5 rounded-lg bg-surface-elevated hover:bg-white/10 border border-white/10 text-zinc-400 hover:text-white transition-colors"
          >
            <ChevronLeft className="w-4 h-4" />
          </button>
          <button
            onClick={() => scroll('right')}
            className="p-1.5 rounded-lg bg-surface-elevated hover:bg-white/10 border border-white/10 text-zinc-400 hover:text-white transition-colors"
          >
            <ChevronRight className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Cards Scroll Row */}
      <div
        ref={rowRef}
        className="flex items-start space-x-5 overflow-x-auto no-scrollbar py-2 -my-2"
      >
        {section.items.map((item) => (
          <PosterCard
            key={item.id}
            item={item}
            onClick={onSelectItem}
            onPlayDirect={onPlayItem}
          />
        ))}
      </div>
    </div>
  );
};
