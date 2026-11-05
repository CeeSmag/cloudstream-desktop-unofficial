import React from 'react';
import { Info, Play } from 'lucide-react';
import { WatchProgress } from '../types';
import { useTitleDetails } from '../hooks/useTitleDetails';
import { findEpisodeForKey } from '../playback/episodeLookup';
import { resumeFacts, resumePercent } from '../utils/resumeHero';

interface ResumeHeroProps {
  item: WatchProgress;
  /** Plays the episode or movie from where it was left. */
  onResume: (item: WatchProgress) => void;
  /** Opens the title page. */
  onDetails: (item: WatchProgress) => void;
}

/** The Home banner for the title the viewer was last watching. */
export const ResumeHero: React.FC<ResumeHeroProps> = ({ item, onResume, onDetails }) => {
  const { details, failed } = useTitleDetails(item.provider, item.detail_url);
  // Until the title is known nothing that would change afterwards is shown (the episode's own
  // picture and name, for instance), so the banner does not flash one look and then another.
  const loading = !details && !failed;
  const backdrop = loading ? null : details?.background_url || details?.poster_url || item.poster_url;
  const poster = loading ? null : details?.poster_url || item.poster_url;
  const episodeName = details && item.episode != null ? findEpisodeForKey(details, item.media_id)?.name : null;
  const percent = resumePercent(item);
  const facts = [details?.media_type, details?.year, episodeName].filter(Boolean).join(' · ');

  return (
    <div className="relative w-full h-[clamp(17rem,48vh,36rem)] overflow-hidden">
      <div className="absolute inset-0">
        {backdrop && (
          <img
            src={backdrop}
            alt=""
            decoding="async"
            className="w-full h-full object-cover object-top opacity-40"
          />
        )}
        <div className="absolute inset-0 bg-gradient-to-t from-midnight via-midnight/60 to-transparent" />
        <div className="absolute inset-0 bg-gradient-to-r from-midnight via-midnight/80 to-transparent" />
      </div>

      <div className="relative h-full max-w-[100rem] mx-auto page-gutter flex items-center gap-4 sm:gap-6 py-4 sm:py-8 z-10">
        {loading && (
          <>
            <div className="hidden sm:block h-full aspect-[2/3] flex-none rounded-xl bg-white/5 animate-pulse" />
            <div
              className="hidden sm:block self-stretch w-0.5 my-2 rounded-full bg-white/70 flex-none"
              aria-hidden="true"
            />
          </>
        )}
        {poster && (
          <>
            <img
              src={poster}
              alt={details?.name ?? item.title}
              className="hidden sm:block h-full max-h-full w-auto aspect-[2/3] flex-none object-cover rounded-xl border border-white/10 shadow-2xl"
            />
            <div
              className="hidden sm:block self-stretch w-0.5 my-2 rounded-full bg-white/70 flex-none"
              aria-hidden="true"
            />
          </>
        )}
        <div className="min-w-0 flex-1 flex flex-col justify-end self-end">
          <div className="flex flex-wrap items-center gap-2 mb-3">
            <span className="px-2.5 py-0.5 rounded-md bg-accent/20 text-accent font-semibold text-xs border border-accent/30 tracking-wide uppercase">
              Continue Watching
            </span>
            <span className="text-zinc-300 text-xs font-medium">{resumeFacts(item)}</span>
          </div>

          {loading ? (
            <div
              className="h-9 sm:h-12 w-72 max-w-full rounded-lg bg-white/10 animate-pulse mb-3"
              aria-label="Loading"
            />
          ) : (
            <h1 className="text-2xl sm:text-4xl lg:text-5xl font-extrabold text-white tracking-tight drop-shadow-md mb-2 max-w-2xl break-words">
              {details?.name ?? item.title}
            </h1>
          )}
          {facts && <p className="text-zinc-400 text-xs font-medium mb-2">{facts}</p>}
          {details?.plot && (
            <p className="text-zinc-300 text-sm leading-relaxed max-w-2xl line-clamp-2 mb-4">{details.plot}</p>
          )}

          {percent > 0 && (
            <div
              className="h-1 w-full max-w-sm rounded-full bg-white/20 mb-4"
              role="progressbar"
              aria-valuenow={percent}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <div className="h-full rounded-full bg-accent" style={{ width: `${Math.max(percent, 3)}%` }} />
            </div>
          )}

          <div className="flex flex-wrap items-center gap-3">
            <button
              onClick={() => onResume(item)}
              className="flex items-center space-x-2.5 px-6 py-3 rounded-xl bg-accent hover:bg-accent-hover text-white text-sm font-semibold transition-colors duration-150 shadow-lg glow-accent"
            >
              <Play className="w-4 h-4 fill-current" />
              <span>{percent > 0 ? 'Resume' : 'Play'}</span>
            </button>
            <button
              onClick={() => onDetails(item)}
              className="flex items-center space-x-2.5 px-6 py-3 rounded-xl bg-white/10 hover:bg-white/15 text-white text-sm font-semibold transition-colors duration-150 border border-white/10"
            >
              <Info className="w-4 h-4" />
              <span>Details</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
