import React from 'react';
import { Play } from 'lucide-react';
import { HomeItem } from '../types';

interface PosterCardProps {
  item: HomeItem;
  onClick: (item: HomeItem) => void;
  onPlayDirect?: (item: HomeItem) => void;
}

export const PosterCard: React.FC<PosterCardProps> = ({ item, onClick, onPlayDirect }) => {
  // A poster that fails to load falls back to the same placeholder as a missing one.
  const [imageFailed, setImageFailed] = React.useState(false);

  return (
    <div
      onClick={() => onClick(item)}
      className="group relative w-full cursor-pointer select-none"
    >
      {/* Poster Image Container */}
      <div className="relative aspect-[2/3] w-full rounded-2xl overflow-hidden bg-surface-elevated border border-white/5 shadow-lg group-hover:border-accent/40 group-hover:shadow-accent/20 transition-all duration-300 transform group-hover:-translate-y-1">
        {item.poster_url && !imageFailed ? (
          <img
            onError={() => setImageFailed(true)}
            src={item.poster_url}
            alt={item.name}
            className="w-full h-full object-cover group-hover:scale-105 transition-transform duration-500"
            loading="lazy"
          />
        ) : (
          <div className="w-full h-full flex flex-col items-center justify-center p-4 bg-gradient-to-b from-surface-elevated to-midnight text-zinc-500">
            <span className="text-3xl mb-2">🎬</span>
            <span className="text-xs text-center line-clamp-2">{item.name}</span>
          </div>
        )}

        {/* Gradient Shadow Vignette */}
        <div className="absolute inset-0 bg-gradient-to-t from-midnight/90 via-transparent to-transparent opacity-60 group-hover:opacity-40 transition-opacity" />

        {/* Play Button Overlay */}
        <div className="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity duration-300 bg-midnight/30 backdrop-blur-[2px]">
          <button
            onClick={(e) => {
              e.stopPropagation();
              onPlayDirect ? onPlayDirect(item) : onClick(item);
            }}
            className="w-12 h-12 rounded-full bg-accent text-white flex items-center justify-center shadow-xl glow-accent transform scale-90 group-hover:scale-100 transition-transform duration-200"
          >
            <Play className="w-5 h-5 fill-current ml-0.5" />
          </button>
        </div>

        {/* Top Badges */}
        <div className="absolute top-2.5 left-2.5 right-2.5 flex items-center justify-between text-[11px] font-semibold">
          {item.media_type && (
            <span className="px-2 py-0.5 rounded-md bg-midnight/80 backdrop-blur-md text-zinc-300 border border-white/10 uppercase tracking-wider text-[10px]">
              {item.media_type}
            </span>
          )}
          {item.year && (
            <span className="px-2 py-0.5 rounded-md bg-midnight/80 backdrop-blur-md text-zinc-400 border border-white/10">
              {item.year}
            </span>
          )}
        </div>

      </div>

      {/* Title & Metadata */}
      <div className="mt-2.5 px-0.5">
        <h3 className="text-sm font-medium text-zinc-200 group-hover:text-white line-clamp-1 transition-colors">
          {item.name}
        </h3>
        <p className="text-xs text-zinc-500 font-normal truncate mt-0.5">
          {item.provider}
        </p>
      </div>
    </div>
  );
};
