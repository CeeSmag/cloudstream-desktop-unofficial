import React from 'react';
import { Star } from 'lucide-react';
import { BrowseItem } from '../types';
import { formatRating } from '../utils/browseFilters';

interface BrowseCardProps {
  item: BrowseItem;
  onSelect: (item: BrowseItem) => void;
}

export const BrowseCard: React.FC<BrowseCardProps> = React.memo(({ item, onSelect }) => {
  const [imageFailed, setImageFailed] = React.useState(false);
  const formattedScore = formatRating(item.rating);

  return (
    <div
      onClick={() => onSelect(item)}
      className="group relative w-full cursor-pointer select-none card-contain"
    >
      <div className="relative aspect-[2/3] w-full rounded-2xl overflow-hidden bg-surface-elevated border border-white/5 shadow-md group-hover:border-accent/40 transition-transform duration-200 transform group-hover:-translate-y-0.5">
        {item.poster_url && !imageFailed ? (
          <img
            onError={() => setImageFailed(true)}
            src={item.poster_url}
            alt={item.name}
            className="w-full h-full object-cover"
            loading="lazy"
            decoding="async"
          />
        ) : (
          <div className="w-full h-full flex flex-col items-center justify-center p-3 bg-gradient-to-b from-surface-elevated to-midnight text-zinc-500">
            <span className="text-2xl mb-1">🎬</span>
            <span className="text-xs text-center break-words line-clamp-2">{item.name}</span>
          </div>
        )}

        <div className="absolute inset-0 bg-gradient-to-t from-midnight/85 via-transparent to-transparent opacity-60 pointer-events-none" />

        {/* Top Badges (solid semi-opaque background avoids 120 GPU backdrop-blur passes per frame) */}
        <div className="absolute top-2 left-2 right-2 flex items-center justify-between text-[10px] font-semibold pointer-events-none">
          <span className="px-1.5 py-0.5 rounded-md bg-midnight/90 text-zinc-200 border border-white/10 uppercase tracking-wider">
            {item.media_type === 'tv' ? 'TV' : 'Movie'}
          </span>
          {formattedScore && (
            <span className="flex items-center gap-0.5 px-1.5 py-0.5 rounded-md bg-midnight/90 text-amber-300 border border-amber-500/40 font-bold">
              <Star className="w-2.5 h-2.5 fill-current" />
              <span>{formattedScore}</span>
            </span>
          )}
        </div>
      </div>

      <div className="mt-2 px-0.5">
        <h3 className="text-xs sm:text-sm font-medium text-zinc-200 group-hover:text-white break-words line-clamp-1 transition-colors">
          {item.name}
        </h3>
        <p className="text-[11px] text-zinc-500 font-normal truncate mt-0.5">
          {item.year ?? 'Unknown Year'}
        </p>
      </div>
    </div>
  );
});
