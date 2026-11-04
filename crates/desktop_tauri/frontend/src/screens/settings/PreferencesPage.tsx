import React from 'react';
import { AudioLines, Check, SkipForward, Subtitles } from 'lucide-react';
import { usePreferences } from '../../hooks/usePreferences';
import { SubtitleLanguages } from './SubtitleLanguages';

interface PreferencesPageProps {
  onError: (error: unknown) => void;
}

/** The value of the "no preference" entry of the list. */
const STREAM_DEFAULT = '';

/** What the viewer prefers: the audio language, and which subtitle languages get a flag under each source in the player. */
export const PreferencesPage: React.FC<PreferencesPageProps> = ({ onError }) => {
  const { preferences, busy, saved, chooseAudio, chooseSubtitles, chooseAutoSubtitles, choosePlaybackOption } =
    usePreferences(onError);

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

      <div className="p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
        <h2 className="text-lg font-bold text-white flex items-center gap-2">
          <SkipForward className="w-5 h-5 text-accent flex-none" />
          <span>Episodes</span>
        </h2>
        <label className="flex items-start gap-3 text-sm text-zinc-200 cursor-pointer">
          <input
            type="checkbox"
            checked={preferences?.autoplay_next ?? true}
            disabled={busy || preferences === null}
            onChange={(event) => void choosePlaybackOption('autoplay_next', event.target.checked)}
            className="mt-0.5 h-4 w-4 accent-accent"
          />
          <span>
            Play the next episode automatically
            <span className="block text-xs text-zinc-400">
              When an episode reaches its end, the next one starts by itself.
            </span>
          </span>
        </label>
        <label className="flex items-start gap-3 text-sm text-zinc-200 cursor-pointer">
          <input
            type="checkbox"
            checked={preferences?.skip_lookup ?? true}
            disabled={busy || preferences === null}
            onChange={(event) => void choosePlaybackOption('skip_lookup', event.target.checked)}
            className="mt-0.5 h-4 w-4 accent-accent"
          />
          <span>
            Look up intro and credits times online
            <span className="block text-xs text-zinc-400">
              Shows a Skip Intro button when a community database knows where the intro is. This tells that site which
              title is playing (its IMDb, TMDb or MyAnimeList id), nothing else.
            </span>
          </span>
        </label>
      </div>

      <div className="p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
        <div className="space-y-1">
          <h2 className="text-lg font-bold text-white flex items-center gap-2">
            <Subtitles className="w-5 h-5 text-accent flex-none" />
            <span>Preferred subtitles</span>
          </h2>
          <p className="text-xs text-zinc-400">
            Choose up to three languages. In the player's Sources menu, each source shows a flag with the first two
            letters of the language for every one of these languages it has subtitles in. A source without subtitles
            in a language shows no flag for it. Flags appear once the source has been checked.
          </p>
        </div>
        <label className="flex items-start gap-3 text-sm text-zinc-200 cursor-pointer">
          <input
            type="checkbox"
            checked={preferences?.auto_subtitles ?? true}
            disabled={busy || preferences === null}
            onChange={(event) => void chooseAutoSubtitles(event.target.checked)}
            className="mt-0.5 h-4 w-4 accent-accent"
          />
          <span>
            Turn on subtitles automatically
            <span className="block text-xs text-zinc-400">
              When a video has subtitles in one of these languages, the first language in your list is shown. Subtitles you
              pick or switch off yourself are left alone.
            </span>
          </span>
        </label>
        <SubtitleLanguages
          languages={preferences?.languages ?? []}
          chosen={preferences?.preferred_subtitles ?? []}
          disabled={busy || preferences === null}
          onChange={(codes) => void chooseSubtitles(codes)}
        />
      </div>
    </div>
  );
};
