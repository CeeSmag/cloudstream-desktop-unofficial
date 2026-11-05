import React from 'react';
import { Bookmark as BookmarkIcon, Trash2, Clock } from 'lucide-react';
import { Bookmark, HomeItem, WatchProgress } from '../types';
import { PosterCard } from '../components/PosterCard';
import { ContinueWatchingCard } from '../components/ContinueWatchingCard';
import { continueKey } from '../utils/continueWatching';

interface LibraryScreenProps {
  bookmarks: Bookmark[];
  continueWatching: WatchProgress[];
  onSelectItem: (item: HomeItem) => void;
  /** Opens the title page of a Continue Watching entry. */
  onOpenContinue: (progress: WatchProgress) => void;
  onRemoveContinue: (progress: WatchProgress) => void;
  onClearContinue: () => void;
  onRemoveBookmark: (provider: string, url: string) => void;
}

export const LibraryScreen: React.FC<LibraryScreenProps> = ({
  bookmarks,
  continueWatching,
  onSelectItem,
  onOpenContinue,
  onRemoveContinue,
  onClearContinue,
  onRemoveBookmark,
}) => {
  const [activeTab, setActiveTab] = React.useState('All');

  const tabs = ['All', 'Watching', 'Plan to Watch', 'Completed'];

  const filteredBookmarks = React.useMemo(() => {
    if (activeTab === 'All') return bookmarks;
    return bookmarks.filter(
      (b) => b.status.toLowerCase() === activeTab.toLowerCase().replace(/\s+/g, '')
    );
  }, [bookmarks, activeTab]);

  return (
    <div className="flex-1 overflow-y-auto page-gutter py-6 sm:py-8 max-w-[100rem] mx-auto w-full select-none space-y-8 sm:space-y-10">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4 border-b border-white/10 pb-5">
        <div>
          <h1 className="text-2xl sm:text-2xl sm:text-3xl font-extrabold text-white tracking-tight">Your Library</h1>
          <p className="text-xs text-zinc-400 mt-1">
            {bookmarks.length} saved titles • {continueWatching.length} in progress
          </p>
        </div>

        {/* Category Tabs */}
        <div className="flex items-center gap-1.5 p-1 max-w-full overflow-x-auto no-scrollbar rounded-xl bg-surface-elevated border border-white/5">
          {tabs.map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`flex-none whitespace-nowrap px-3.5 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                activeTab === tab
                  ? 'bg-accent text-white font-semibold shadow-sm'
                  : 'text-zinc-400 hover:text-white'
              }`}
            >
              {tab}
            </button>
          ))}
        </div>
      </div>

      {/* Continue Watching Section */}
      {continueWatching.length > 0 && (
        <div>
          <div className="flex items-center justify-between gap-3 mb-4">
            <h2 className="text-lg font-bold text-white flex items-center gap-2">
              <Clock className="w-4 h-4 text-accent" />
              <span>Continue Watching</span>
            </h2>
            <ClearAllButton onConfirm={onClearContinue} />
          </div>

          <div className="flex items-start gap-4 overflow-x-auto no-scrollbar py-2">
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

      {/* Bookmarks Grid */}
      <div>
        <h2 className="text-lg font-bold text-white mb-4">Saved Bookmarks</h2>

        {filteredBookmarks.length > 0 ? (
          <div className="poster-grid">
            {filteredBookmarks.map((b) => (
              <div key={`${b.provider}-${b.url}`} className="relative group/card">
                <PosterCard
                  item={{
                    id: b.url,
                    name: b.title,
                    url: b.url,
                    poster_url: b.poster_url,
                    provider: b.provider,
                    media_type: b.status,
                    year: null,
                  }}
                  onClick={onSelectItem}
                />
                {/* Delete Bookmark Icon */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemoveBookmark(b.provider, b.url);
                  }}
                  className="absolute top-2 right-2 p-1.5 rounded-lg bg-midnight/80 hover:bg-rose-500 text-zinc-400 hover:text-white opacity-0 group-hover/card:opacity-100 transition-opacity border border-white/10"
                  title="Remove Bookmark"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            ))}
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center py-20 text-zinc-500">
            <BookmarkIcon className="w-12 h-12 stroke-[1.5] mb-3 text-zinc-600" />
            <p className="text-base font-medium text-zinc-400">No bookmarks in this category</p>
            <p className="text-xs text-zinc-600 mt-1">Browse and save your favorite movies and shows</p>
          </div>
        )}
      </div>
    </div>
  );
};

/** Empties Continue Watching, after asking once, since it also forgets what was kept on disk. */
const ClearAllButton: React.FC<{ onConfirm: () => void }> = ({ onConfirm }) => {
  const [asking, setAsking] = React.useState(false);
  if (!asking) {
    return (
      <button
        onClick={() => setAsking(true)}
        className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-400 hover:text-white hover:bg-white/10 border border-white/10 transition-colors"
      >
        <Trash2 className="w-3.5 h-3.5" />
        Clear all
      </button>
    );
  }
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="text-zinc-400">Remove all from Continue Watching?</span>
      <button
        onClick={() => {
          setAsking(false);
          onConfirm();
        }}
        className="px-3 py-1.5 rounded-lg font-semibold bg-rose-500 hover:bg-rose-600 text-white"
      >
        Yes, clear
      </button>
      <button
        onClick={() => setAsking(false)}
        className="px-3 py-1.5 rounded-lg text-zinc-300 hover:bg-white/10 border border-white/10"
      >
        Cancel
      </button>
    </div>
  );
};
