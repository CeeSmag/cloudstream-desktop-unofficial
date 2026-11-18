import React from 'react';
import { X } from 'lucide-react';
import { LanguageOption } from '../../types';
import { abbreviationOfLanguage, flagOfLanguage } from '../../utils/subtitleFlags';

/** How many subtitle languages can get a flag in the player. */
const MAX_CHOICES = 3;

interface SubtitleLanguagesProps {
  languages: LanguageOption[];
  /** Codes of the chosen languages, in order. */
  chosen: string[];
  disabled: boolean;
  onChange: (codes: string[]) => void;
}

/** The chosen subtitle languages as removable flags, and a list to add another while there is room. */
export const SubtitleLanguages: React.FC<SubtitleLanguagesProps> = ({ languages, chosen, disabled, onChange }) => {
  const chosenLanguages = chosen
    .map((code) => languages.find((language) => language.code === code))
    .filter((language): language is LanguageOption => language !== undefined);
  const available = languages.filter((language) => !chosen.includes(language.code));
  const full = chosen.length >= MAX_CHOICES;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2 min-h-[2.25rem]">
        {chosenLanguages.length === 0 && <span className="text-xs text-zinc-500">No subtitle flags will be shown.</span>}
        {chosenLanguages.map((language) => (
          <span
            key={language.code}
            className="flex items-center gap-1.5 pl-2.5 pr-1.5 py-1 rounded-lg bg-accent/15 border border-accent/40 text-xs font-semibold text-accent"
          >
            <span className="text-base leading-none">{flagOfLanguage(language.name)}</span>
            <span>{abbreviationOfLanguage(language.name)}</span>
            <span className="font-normal text-zinc-300">{language.name}</span>
            <button
              disabled={disabled}
              onClick={() => onChange(chosen.filter((code) => code !== language.code))}
              aria-label={`Remove ${language.name}`}
              className="p-0.5 rounded hover:bg-white/10 text-zinc-300 hover:text-white disabled:opacity-60"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </span>
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <label htmlFor="add-subtitle-language" className="text-sm font-medium text-zinc-200">
          Add a language
        </label>
        <select
          id="add-subtitle-language"
          disabled={disabled || full}
          value=""
          onChange={(event) => event.target.value && onChange([...chosen, event.target.value])}
          className="min-w-[12rem] rounded-xl bg-surface-elevated border border-white/10 px-3 py-2 text-sm text-white disabled:opacity-60"
        >
          <option value="">{full ? `Up to ${MAX_CHOICES} languages` : 'Choose…'}</option>
          {available.map((language) => (
            <option key={language.code} value={language.code}>
              {flagOfLanguage(language.name)} {language.name}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
};
