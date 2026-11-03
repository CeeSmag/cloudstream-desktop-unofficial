import React from 'react';
import { SubtitleFlag } from '../utils/subtitleFlags';

interface SubtitleFlagsProps {
  flags: SubtitleFlag[];
  /** Turns the subtitles of a flag on, or off when its subtitles are already showing. */
  onToggle: (flag: SubtitleFlag) => void;
}

/** A row of up to three flags, each with the first letters of its language, that switch subtitles. */
export const SubtitleFlags: React.FC<SubtitleFlagsProps> = ({ flags, onToggle }) => {
  if (flags.length === 0) return null;
  return (
    <div className="px-2 pb-2 pt-1 border-b border-white/10 mb-1">
      <div className="px-1 pb-1.5 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">Subtitles</div>
      <div className="flex flex-wrap gap-2">
        {flags.map((flag) => (
          <button
            key={flag.code}
            onClick={() => onToggle(flag)}
            title={`${flag.name} subtitles`}
            aria-pressed={flag.selected}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border text-xs font-semibold transition-colors ${
              flag.selected
                ? 'bg-accent/20 border-accent text-accent'
                : 'bg-white/5 border-white/10 text-zinc-300 hover:bg-white/10'
            }`}
          >
            <span className="text-base leading-none">{flag.flag}</span>
            <span>{flag.abbreviation}</span>
          </button>
        ))}
      </div>
    </div>
  );
};
