import React from 'react';
import { Info, ChevronRight, ChevronLeft, Clock } from 'lucide-react';
import { HomeItem, HomeSection, WatchProgress } from '../types';
import { PosterCard } from '../components/PosterCard';
import { ContinueWatchingCard } from '../components/ContinueWatchingCard';
import { ResumeHero } from '../components/ResumeHero';
import { continueKey } from '../utils/continueWatching';
import { pickResumeHero } from '../utils/resumeHero';

interface HomeScreenProps {
  sections: HomeSection[];
  onSelectItem: (item: HomeItem) => void;
  loading: boolean;
  continueWatching: WatchProgress[];
  /** Opens the title page of a Continue Watching entry. */
  onOpenContinue: (progress: WatchProgress) => void;
  /** Plays a Continue Watching entry from where it was left. */
  onResumeContinue: (progress: WatchProgress) => void;
  onRemoveContinue: (progress: WatchProgress) => void;
}

export const HomeScreen: React.FC<HomeScreenProps> = ({
  sections,
  onSelectItem,
  loading,
  continueWatching,
  onOpenContinue,
  onResumeContinue,
  onRemoveContinue,
}) => {
  const heroItem = sections[0]?.items[0];
  const resumeItem = pickResumeHero(continueWatching);

  return (
    <div className="flex-1 overflow-y-auto pb-20 select-none">
      {/* Hero Banner: what the viewer was last watching, else the first title of the feed */}
      {resumeItem && <ResumeHero item={resumeItem} onResume={onResumeContinue} onDetails={onOpenContinue} />}
      {!resumeItem && heroItem && (
        <div className="relative w-full h-[clamp(17rem,48vh,36rem)] overflow-hidden">
          {/* Backdrop Image */}
          <div className="absolute inset-0">
            {heroItem.poster_url && (
              <img
                src={heroItem.poster_url}
                alt={heroItem.name}
                decoding="async"
                className="w-full h-full object-cover object-top opacity-40"
              />
            )}
            {/* Gradient Overlays */}
            <div className="absolute inset-0 bg-gradient-to-t from-midnight via-midnight/60 to-transparent" />
            <div className="absolute inset-0 bg-gradient-to-r from-midnight via-midnight/80 to-transparent" />
          </div>

          {/* Hero Content */}
          <div className="relative h-full max-w-[100rem] mx-auto page-gutter flex flex-col justify-end pb-6 sm:pb-12 z-10">
            <div className="flex flex-wrap items-center gap-2 mb-3">
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

            <h1 className="text-2xl sm:text-4xl lg:text-5xl font-extrabold text-white tracking-tight drop-shadow-md mb-3 max-w-2xl break-words">
              {heroItem.name}
            </h1>

            {/* Action Buttons */}
            <div className="flex flex-wrap items-center gap-3">
              <button
                onClick={() => onSelectItem(heroItem)}
                className="flex items-center space-x-2.5 px-6 py-3 rounded-xl bg-accent hover:bg-accent-hover text-white text-sm font-semibold transition-colors duration-150 shadow-lg glow-accent"
              >
                <Info className="w-4 h-4" />
                <span>Details</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Sections / Shelves */}
      <div className="max-w-[100rem] mx-auto page-gutter space-y-8 sm:space-y-10 mt-4">
        {continueWatching.length > 0 && (
          <div>
            <h2 className="text-lg sm:text-xl font-bold text-white tracking-tight flex items-center gap-2 mb-4">
              <Clock className="w-4 h-4 text-accent" />
              <span>Continue Watching</span>
            </h2>
            <div className="flex items-start gap-4 overflow-x-auto no-scrollbar py-2 -my-2">
              {continueWatching.map((item) => (
                <ContinueWatchingCard
                  key={continueKey(item)}
                  item={item}
                  onOpen={onOpenContinue}
                  onRemove={onRemoveContinue}
                />
              ))}
            </div>
          </div>
        )}
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
}> = ({ section, onSelectItem }) => {
  const rowRef = React.useRef<HTMLDivElement>(null);

  const scroll = (direction: 'left' | 'right') => {
    if (rowRef.current) {
      const page = rowRef.current.clientWidth * 0.8;
      rowRef.current.scrollBy({ left: direction === 'left' ? -page : page, behavior: 'smooth' });
    }
  };

  return (
    <div className="relative group/shelf">
      {/* Section Title Header */}
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg sm:text-xl font-bold text-white tracking-tight flex items-center gap-2 min-w-0">
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
        className="flex items-start gap-3 sm:gap-5 overflow-x-auto no-scrollbar py-2 -my-2"
      >
        {section.items.map((item) => (
          <div key={item.id} className="shelf-item">
            <PosterCard item={item} onClick={onSelectItem} />
          </div>
        ))}
      </div>
    </div>
  );
};
