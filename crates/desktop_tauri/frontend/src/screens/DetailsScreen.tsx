import React from 'react';
import { ArrowLeft, Play, BookmarkPlus, Check, Calendar, Clock, Star, Tag } from 'lucide-react';
import { Episode, MediaDetails, ResumePointer } from '../types';
import { api } from '../services/api';
import { CastRow } from '../components/details/CastRow';
import { EpisodeSection } from '../components/details/EpisodeSection';
import { groupBySeason, seasonNumbers, sortEpisodes } from '../utils/episodeGroups';
import { findEpisodeForKey, movieAsEpisode } from '../playback/episodeLookup';
import { episodeBadgeText, formatRemainingTime } from '../utils/continueWatching';
import { EpisodeMark, marksByKey } from '../utils/episodeMarks';

interface DetailsScreenProps {
  details: MediaDetails;
  onBack: () => void;
  onPlayEpisode: (ep: Episode) => void;
  isBookmarked: boolean;
  onToggleBookmark: () => void;
}

const primaryButton =
  'flex items-center space-x-2.5 px-6 py-3 rounded-xl bg-accent hover:bg-accent-hover text-white text-sm font-semibold transition-all duration-200 shadow-lg glow-accent';

const secondaryButton =
  'flex items-center space-x-2.5 px-6 py-3 rounded-xl bg-white/10 hover:bg-white/15 text-white text-sm font-semibold transition-all duration-200 border border-white/10';

const Chip: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <span className="flex items-center space-x-1 text-zinc-400 text-xs">{children}</span>
);

/** The page of a title: its picture, facts, synopsis, how to start watching, cast and episodes. */
export const DetailsScreen: React.FC<DetailsScreenProps> = ({
  details,
  onBack,
  onPlayEpisode,
  isBookmarked,
  onToggleBookmark,
}) => {
  const [chosenSeason, setChosenSeason] = React.useState<number | null>(null);
  const [resumePointer, setResumePointer] = React.useState<ResumePointer | null>(null);

  React.useEffect(() => {
    let active = true;
    api
      .getResumePointer(details.provider, details.url)
      .then((pointer) => {
        if (active) setResumePointer(pointer);
      })
      .catch(() => {
        if (active) setResumePointer(null);
      });
    return () => {
      active = false;
    };
  }, [details.provider, details.url]);

  const [marks, setMarks] = React.useState<Map<string, EpisodeMark>>(new Map());

  React.useEffect(() => {
    let active = true;
    api
      .getShowProgress(details.provider, details.url)
      .then((list) => {
        if (active) setMarks(marksByKey(list));
      })
      .catch(() => {
        if (active) setMarks(new Map());
      });
    return () => {
      active = false;
    };
  }, [details.provider, details.url]);

  const resumeEpisode = React.useMemo(() => {
    if (!resumePointer) return null;
    return findEpisodeForKey(details, resumePointer.media_key);
  }, [details, resumePointer]);

  React.useEffect(() => {
    if (resumeEpisode?.season != null) {
      setChosenSeason(resumeEpisode.season);
    }
  }, [resumeEpisode]);

  const groups = React.useMemo(() => groupBySeason(details.episodes), [details.episodes]);
  const seasons = seasonNumbers(groups);
  const season = chosenSeason !== null && groups.has(chosenSeason) ? chosenSeason : (seasons[0] ?? 1);
  const seasonEpisodes = groups.get(season) ?? [];
  const firstEpisode = sortEpisodes(seasonEpisodes, true)[0];
  const movie = details.episodes.length === 0 ? movieAsEpisode(details) : null;
  const playTarget = firstEpisode ?? movie;
  const playLabel = firstEpisode ? `Play S${season} E${firstEpisode.episode ?? 1}` : 'Play';

  const resumeTimeLeft = resumePointer
    ? formatRemainingTime(resumePointer.position_ms, resumePointer.duration_ms)
    : null;
  const resumeBadge = resumeEpisode ? episodeBadgeText(resumeEpisode) : null;
  const resumeLabel = resumeBadge
    ? `Resume ${resumeBadge} (${resumeTimeLeft})`
    : resumeTimeLeft && resumeTimeLeft !== '0m left'
      ? `Resume (${resumeTimeLeft})`
      : 'Resume';

  return (
    <div className="relative flex-1 overflow-y-auto pb-24 select-none">
      {details.background_url && (
        <div className="absolute inset-x-0 top-0 h-[28rem] overflow-hidden pointer-events-none">
          <img src={details.background_url} alt="" className="w-full h-full object-cover opacity-30" />
          <div className="absolute inset-0 bg-gradient-to-b from-midnight/40 via-midnight/70 to-midnight" />
        </div>
      )}

      <div className="sticky top-0 h-14 page-gutter flex items-center z-30 bg-gradient-to-b from-midnight/90 to-transparent backdrop-blur-sm">
        <button
          onClick={onBack}
          className="flex items-center space-x-2 px-3 py-1.5 rounded-xl bg-white/10 hover:bg-white/20 text-white text-xs font-medium border border-white/10 transition-colors"
        >
          <ArrowLeft className="w-4 h-4" />
          <span>Back</span>
        </button>
      </div>

      <div className="relative max-w-[100rem] mx-auto page-gutter pt-2">
        <div className="flex flex-col sm:flex-row items-center sm:items-start gap-6 sm:gap-8">
          <div className="w-40 sm:w-48 lg:w-56 flex-shrink-0 rounded-2xl overflow-hidden bg-surface-elevated border border-white/10 shadow-2xl">
            {details.poster_url ? (
              <img src={details.poster_url} alt={details.name} className="w-full h-auto aspect-[2/3] object-cover" />
            ) : (
              <div className="w-full aspect-[2/3] flex items-center justify-center bg-surface-elevated text-zinc-500">
                No Poster
              </div>
            )}
          </div>

          <div className="flex-1 min-w-0 w-full">
            <div className="flex flex-wrap items-center gap-2.5 mb-2.5">
              {details.media_type && (
                <span className="px-2.5 py-0.5 rounded-md bg-accent/20 text-accent font-semibold text-xs border border-accent/30 uppercase tracking-wide">
                  {details.media_type}
                </span>
              )}
              <span className="px-2.5 py-0.5 rounded-md bg-white/10 text-zinc-300 font-medium text-xs border border-white/10">
                {details.provider}
              </span>
              {details.year && (
                <Chip>
                  <Calendar className="w-3.5 h-3.5" />
                  <span>{details.year}</span>
                </Chip>
              )}
              {details.rating != null && (
                <Chip>
                  <Star className="w-3.5 h-3.5 text-amber-400 fill-amber-400" />
                  <span>{details.rating.toFixed(1)}</span>
                </Chip>
              )}
              {details.duration_minutes != null && (
                <Chip>
                  <Clock className="w-3.5 h-3.5" />
                  <span>{details.duration_minutes} min</span>
                </Chip>
              )}
            </div>

            <h1 className="text-2xl sm:text-3xl lg:text-4xl font-extrabold text-white tracking-tight mb-4 break-words">
              {details.name}
            </h1>

            {details.plot && (
              <p className="text-zinc-300 text-sm leading-relaxed max-w-3xl mb-6">{details.plot}</p>
            )}

            {details.tags.length > 0 && (
              <div className="flex flex-wrap gap-2 mb-6">
                {details.tags.map((tag) => (
                  <span
                    key={tag}
                    className="flex items-center space-x-1 px-2.5 py-1 rounded-lg bg-white/5 border border-white/10 text-zinc-300 text-xs"
                  >
                    <Tag className="w-3 h-3 text-zinc-500" />
                    <span>{tag}</span>
                  </span>
                ))}
              </div>
            )}

            <div className="flex flex-wrap items-center gap-3">
              {resumeEpisode && (
                <button onClick={() => onPlayEpisode(resumeEpisode)} className={primaryButton}>
                  <Play className="w-4 h-4 fill-current" />
                  <span>{resumeLabel}</span>
                </button>
              )}
              {playTarget && (!resumeEpisode || resumeEpisode.data !== playTarget.data) && (
                <button
                  onClick={() => onPlayEpisode(playTarget)}
                  className={resumeEpisode ? secondaryButton : primaryButton}
                >
                  <Play className="w-4 h-4 fill-current" />
                  <span>{playLabel}</span>
                </button>
              )}
              <button
                onClick={onToggleBookmark}
                className={`flex items-center space-x-2 px-5 py-3 rounded-xl border text-sm font-medium transition-colors ${
                  isBookmarked
                    ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-400'
                    : 'bg-white/10 border-white/10 text-white hover:bg-white/15'
                }`}
              >
                {isBookmarked ? <Check className="w-4 h-4 text-emerald-400" /> : <BookmarkPlus className="w-4 h-4" />}
                <span>{isBookmarked ? 'Saved in Library' : 'Save to Library'}</span>
              </button>
            </div>
          </div>
        </div>

        {details.episodes.length > 0 && (
          <EpisodeSection
            seasons={seasons}
            selectedSeason={season}
            onSelectSeason={setChosenSeason}
            episodes={seasonEpisodes}
            fallbackPoster={details.poster_url}
            onPlay={onPlayEpisode}
            highlightedKey={resumePointer?.media_key}
            marks={marks}
          />
        )}

        <CastRow cast={details.cast} />
      </div>
    </div>
  );
};
