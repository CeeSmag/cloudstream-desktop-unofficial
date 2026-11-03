import React from 'react';
import { AudioLines, Check } from 'lucide-react';
import { usePreferences } from '../../hooks/usePreferences';

interface PreferencesPageProps {
  onError: (error: unknown) => void;
}

/** The value of the "no preference" entry of the list. */
const STREAM_DEFAULT = '';

/** What the viewer prefers: for now, which audio language plays when a stream offers several. */
export const PreferencesPage: React.FC<PreferencesPageProps> = ({ onError }) => {
  const { preferences, busy, saved, chooseAudio } = usePreferences(onError);

  return (
    <div className="space-y-8 sm:space-y-10">
      <div className="p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
        <div className="space-y-1">
          <h2 className="text-lg font-bold text-white flex items-center gap-2">
            <AudioLines className="w-5 h-5 text-accent flex-none" />
            <span>Preferred audio track</span>
          </h2>
          <p className="text-xs text-zinc-400">
            When a stream offers several audio tracks, the one in this language plays first. If the stream has no track in
            this language, its own default track plays. You can still change the track while watching.
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <label htmlFor="preferred-audio" className="text-sm font-medium text-zinc-200">
            Language
          </label>
          <select
            id="preferred-audio"
            disabled={busy || preferences === null}
            value={preferences?.preferred_audio ?? STREAM_DEFAULT}
            onChange={(event) => void chooseAudio(event.target.value === STREAM_DEFAULT ? null : event.target.value)}
            className="min-w-[12rem] rounded-xl bg-surface-elevated border border-white/10 px-3 py-2 text-sm text-white disabled:opacity-60"
          >
            <option value={STREAM_DEFAULT}>Stream default (no preference)</option>
            {preferences?.languages.map((language) => (
              <option key={language.code} value={language.code}>
                {language.name}
              </option>
            ))}
          </select>
          <span role="status" className="inline-flex items-center gap-1 text-xs text-emerald-400 min-h-[1rem]">
            {saved && (
              <>
                <Check className="w-3.5 h-3.5" /> Saved
              </>
            )}
          </span>
        </div>
      </div>
    </div>
  );
};
