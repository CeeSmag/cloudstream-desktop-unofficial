import React from 'react';
import { LanguageFlag } from '../utils/subtitleFlags';

interface SubtitleFilterProps {
  /** The viewer's preferred subtitle languages. */
  languages: LanguageFlag[];
  /** Codes of the toggled languages. */
  toggled: string[];
  onToggle: (code: string) => void;
  shown: number;
  total: number;
}

/** Toggles above the list of sources: only sources with subtitles in every toggled language are listed. */
export const SubtitleFilter: React.FC<SubtitleFilterProps> = ({ languages, toggled, onToggle, shown, total }) => {
  if (languages.length === 0) return null;
  return (
    <div className="px-2 pb-2 pt-1 border-b border-white/10 mb-1">
      <div className="flex items-center justify-between px-1 pb-1.5 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
        <span>Only with subtitles</span>
        <span className="font-medium normal-case tracking-normal text-zinc-500">
          {shown} of {total}
        </span>
      </div>
      <div className="flex flex-wrap gap-2">
        {languages.map((language) => {
          const active = toggled.includes(language.code);
          return (
            <button
              key={language.code}
              onClick={() => onToggle(language.code)}
              aria-pressed={active}
              title={`Only sources with ${language.name} subtitles`}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border text-xs font-semibold transition-colors ${
                active
                  ? 'bg-accent/20 border-accent text-accent'
                  : 'bg-white/5 border-white/10 text-zinc-300 hover:bg-white/10'
              }`}
            >
              <span className="text-base leading-none">{language.flag}</span>
              <span>{language.abbreviation}</span>
            </button>
          );
        })}
      </div>
      {toggled.length > 0 && shown === 0 && (
        <p className="px-1 pt-2 text-[11px] text-zinc-500">
          No source has these subtitles yet. Sources that are still being checked show up here once they are.
        </p>
      )}
    </div>
  );
};
