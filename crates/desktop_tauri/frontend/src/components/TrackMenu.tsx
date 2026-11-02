import React from 'react';
import type { LucideIcon } from 'lucide-react';
import { MediaTrack } from '../types';

interface TrackMenuProps {
  /** Heading of the dropdown and tooltip of the button. */
  label: string;
  icon: LucideIcon;
  tracks: MediaTrack[];
  /** Adds an "Off" entry, used for subtitles. */
  allowOff: boolean;
  open: boolean;
  onToggle: () => void;
  onSelect: (id: number | null) => void;
}

const ITEM_BASE = 'w-full text-left px-3 py-2 text-xs rounded-lg flex items-center justify-between transition-colors';

/** A player button that opens a dropdown listing audio or subtitle tracks. */
export const TrackMenu: React.FC<TrackMenuProps> = ({
  label,
  icon: Icon,
  tracks,
  allowOff,
  open,
  onToggle,
  onSelect,
}) => (
  <div className="relative">
    <button
      onClick={onToggle}
      className={`p-2 sm:p-2.5 rounded-xl border transition-colors backdrop-blur-md ${
        open ? 'bg-accent border-accent text-white' : 'bg-white/10 hover:bg-white/20 border-white/10 text-white'
      }`}
      title={label}
      aria-label={label}
    >
      <Icon className="w-5 h-5" />
    </button>

    {open && (
      <div className="absolute right-0 mt-2 w-56 max-w-[calc(100vw-2rem)] max-h-[60vh] overflow-y-auto rounded-xl glass-dropdown p-2 shadow-2xl z-50">
        <div className="px-3 py-1 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">{label}</div>
        {allowOff && (
          <button onClick={() => onSelect(null)} className={`${ITEM_BASE} text-zinc-300 hover:bg-white/5`}>
            Off
          </button>
        )}
        {tracks.map((track) => (
          <button
            key={track.id}
            onClick={() => onSelect(track.id)}
            className={`${ITEM_BASE} ${
              track.selected ? 'bg-accent/20 text-accent font-semibold' : 'text-zinc-300 hover:bg-white/5'
            }`}
          >
            <span className="truncate">{track.title || track.language || `Track ${track.id}`}</span>
            {track.selected && <span className="w-1.5 h-1.5 flex-none rounded-full bg-accent" />}
          </button>
        ))}
      </div>
    )}
  </div>
);
