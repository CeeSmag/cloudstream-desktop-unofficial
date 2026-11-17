import React from 'react';
import { MemoryStick } from 'lucide-react';
import { api } from '../services/api';
import { VideoCacheSetting } from '../types';
import { describeValue, logEvent } from '../utils/logger';
import { awayText, sizeLabel, splitText } from '../utils/videoCache';

interface VideoCacheCardProps {
  onError: (error: unknown) => void;
}

const SELECT = 'flex-none rounded-lg bg-white/10 px-3 py-2 text-sm text-white outline-none disabled:opacity-60';

/** How much of the playing video is kept in memory, and how much of that lies ahead. */
export const VideoCacheCard: React.FC<VideoCacheCardProps> = ({ onError }) => {
  const [setting, setSetting] = React.useState<VideoCacheSetting | null>(null);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    let live = true;
    api
      .getVideoCache()
      .then((value) => {
        if (live) setSetting(value);
      })
      .catch((error: unknown) => {
        logEvent('error', `cannot read the video cache setting: ${describeValue(error)}`);
        onError(error);
      });
    return () => {
      live = false;
    };
  }, [onError]);

  const choose = async (sizeMb: number, aheadPercent: number) => {
    setBusy(true);
    try {
      logEvent('info', `the viewer chose a ${sizeMb} MB video cache, ${aheadPercent} % ahead`);
      setSetting(await api.setVideoCache(sizeMb, aheadPercent));
    } catch (error) {
      logEvent('error', `cannot set the video cache: ${describeValue(error)}`);
      onError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
      <div className="min-w-0 space-y-1">
        <h2 className="text-lg font-bold text-white flex items-center gap-2">
          <MemoryStick className="w-5 h-5 text-accent flex-none" />
          <span>Video cache</span>
        </h2>
        <p className="text-xs text-zinc-400">
          How much of the playing video is kept in memory, so jumping around in it needs no new download. The ahead
          share is read ahead of where you are; the rest keeps what you just watched. When you leave the player the
          video waits, paused, with half of each; opening it again plays on from memory. Playing another video or
          closing the app frees it. Streams split into many pieces (HLS) are read at most a minute ahead.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm text-zinc-300">
          Size
          <select
            aria-label="Video cache size"
            value={setting?.size_mb ?? ''}
            disabled={busy || setting === null}
            onChange={(event) => setting && void choose(Number(event.target.value), setting.ahead_percent)}
            className={SELECT}
          >
            {setting === null && <option value="">...</option>}
            {setting?.sizes_mb.map((mb) => (
              <option key={mb} value={mb} className="bg-zinc-900">
                {sizeLabel(mb)}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-2 text-sm text-zinc-300">
          Ahead
          <select
            aria-label="Share of the video cache read ahead"
            value={setting?.ahead_percent ?? ''}
            disabled={busy || setting === null}
            onChange={(event) => setting && void choose(setting.size_mb, Number(event.target.value))}
            className={SELECT}
          >
            {setting === null && <option value="">...</option>}
            {setting?.ahead_percents.map((percent) => (
              <option key={percent} value={percent} className="bg-zinc-900">
                {percent} %
              </option>
            ))}
          </select>
        </label>
      </div>
      {setting && (
        <p className="text-xs text-zinc-400">
          While playing: {splitText(setting.size_mb, setting.ahead_percent)}. While the video waits:{' '}
          {awayText(setting.size_mb, setting.ahead_percent)}.
        </p>
      )}
    </div>
  );
};
