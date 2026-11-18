import React from 'react';
import { Info, Trash2 } from 'lucide-react';
import { WatchProgress } from '../types';
import { episodeBadgeText, watchedPercent } from '../utils/continueWatching';

interface ContinueWatchingCardProps {
  item: WatchProgress;
  /** Opens the title page; the card never starts the video itself. */
  onOpen: (item: WatchProgress) => void;
  /** Takes the title out of Continue Watching and forgets what was kept for it. */
  onRemove: (item: WatchProgress) => void;
}

/** A title the viewer started and did not finish, with how far they got. */
export const ContinueWatchingCard: React.FC<ContinueWatchingCardProps> = ({
  item,
  onOpen,
  onRemove,
}) => {
  const percent = watchedPercent(item);
  const badge = episodeBadgeText(item);

  return (
    <div
      onClick={() => onOpen(item)}
      className="group relative flex-shrink-0 w-56 sm:w-64 p-3 rounded-2xl bg-surface-elevated/70 hover:bg-surface-elevated border border-white/5 hover:border-accent/40 cursor-pointer transition-colors duration-150"
    >
      <div className="relative aspect-video rounded-xl overflow-hidden bg-midnight mb-2.5 flex items-center justify-center">
        {item.poster_url ? (
          <img
            src={item.poster_url}
            alt={item.title}
            loading="lazy"
            decoding="async"
            className="w-full h-full object-cover"
          />
        ) : (
          <span className="text-2xl">📺</span>
        )}
        {badge && (
          <div className="absolute top-2 left-2 px-2 py-0.5 rounded-md bg-midnight/90 text-accent font-semibold text-xs border border-accent/30 shadow-md">
            {badge}
          </div>
        )}
        <div className="absolute inset-0 bg-midnight/30 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
          <div className="w-10 h-10 rounded-full bg-accent text-white flex items-center justify-center shadow-lg">
            <Info className="w-4 h-4" />
          </div>
        </div>
        <div className="absolute bottom-0 left-0 right-0 h-1 bg-white/20">
          <div className="h-full bg-accent shadow-sm" style={{ width: `${percent}%` }} />
        </div>
      </div>

      <h4 className="text-sm font-semibold text-white truncate">{item.title}</h4>
      <p className="text-xs text-zinc-400 mt-0.5">
        {badge ? `${badge} • ${percent}% watched` : `${percent}% completed`}
      </p>

      <button
        onClick={(event) => {
          event.stopPropagation();
          onRemove(item);
        }}
        className="absolute top-5 right-5 p-1.5 rounded-lg bg-midnight/80 hover:bg-rose-500 text-zinc-300 hover:text-white opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity border border-white/10"
        title="Remove from Continue Watching"
        aria-label={`Remove ${item.title} from Continue Watching`}
      >
        <Trash2 className="w-3.5 h-3.5" />
      </button>
    </div>
  );
};
