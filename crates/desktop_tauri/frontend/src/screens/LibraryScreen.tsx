import React from 'react';
import { Bookmark as BookmarkIcon, Play, Trash2, Clock } from 'lucide-react';
import { Bookmark, HomeItem, WatchProgress } from '../types';
import { PosterCard } from '../components/PosterCard';

interface LibraryScreenProps {
  bookmarks: Bookmark[];
  continueWatching: WatchProgress[];
  onSelectItem: (item: HomeItem) => void;
  onPlayItem: (item: HomeItem) => void;
  onRemoveBookmark: (provider: string, url: string) => void;
}

export const LibraryScreen: React.FC<LibraryScreenProps> = ({
  bookmarks,
  continueWatching,
  onSelectItem,
  onPlayItem,
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
          <h2 className="text-lg font-bold text-white mb-4 flex items-center gap-2">
            <Clock className="w-4 h-4 text-accent" />
            <span>Continue Watching</span>
          </h2>

          <div className="flex items-start gap-4 overflow-x-auto no-scrollbar py-2">
            {continueWatching.map((item) => (
              <div
                key={item.media_id}
                onClick={() =>
                  onPlayItem({
                    id: item.media_id,
                    name: item.title,
                    url: item.media_id,
                    poster_url: null,
                    provider: 'Local',
                    media_type: null,
                    year: null,
                  })
                }
                className="group relative flex-shrink-0 w-56 sm:w-64 p-3 rounded-2xl bg-surface-elevated/70 hover:bg-surface-elevated border border-white/5 hover:border-accent/40 cursor-pointer transition-all duration-200"
              >
                <div className="relative aspect-video rounded-xl overflow-hidden bg-midnight mb-2.5 flex items-center justify-center">
                  <span className="text-2xl">📺</span>
                  <div className="absolute inset-0 bg-midnight/30 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
                    <div className="w-10 h-10 rounded-full bg-accent text-white flex items-center justify-center shadow-lg">
                      <Play className="w-4 h-4 fill-current ml-0.5" />
                    </div>
                  </div>

                  {/* Progress Bar */}
                  <div className="absolute bottom-0 left-0 right-0 h-1 bg-white/20">
                    <div
                      className="h-full bg-accent shadow-sm"
                      style={{ width: `${Math.round(item.percentage * 100)}%` }}
                    />
                  </div>
                </div>

                <h4 className="text-sm font-semibold text-white truncate">{item.title}</h4>
                <p className="text-xs text-zinc-400 mt-0.5">
                  {Math.round(item.percentage * 100)}% completed
                </p>
              </div>
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
                  onPlayDirect={onPlayItem}
                />
                {/* Delete Bookmark Icon */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemoveBookmark(b.provider, b.url);
                  }}
                  className="absolute top-2 right-2 p-1.5 rounded-lg bg-midnight/80 hover:bg-rose-500 text-zinc-400 hover:text-white opacity-0 group-hover/card:opacity-100 transition-all border border-white/10"
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
