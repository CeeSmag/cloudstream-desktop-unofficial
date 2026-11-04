import React from 'react';
import { ArrowDownAZ, ArrowUpAZ, Play } from 'lucide-react';
import { Episode } from '../../types';
import {
  chunkEpisodes,
  chunkLabel,
  seasonLabel,
  sortEpisodes,
} from '../../utils/episodeGroups';

/** With more seasons than this the tabs become a drop-down. */
const MAX_SEASON_TABS = 6;

interface EpisodeSectionProps {
  seasons: number[];
  selectedSeason: number;
  onSelectSeason: (season: number) => void;
  /** Episodes of the selected season, in the order the provider gave them. */
  episodes: Episode[];
  /** Shown for episodes that have no picture of their own. */
  fallbackPoster: string | null;
  onPlay: (episode: Episode) => void;
  highlightedKey?: string;
}

const pillClass = (active: boolean) =>
  `flex-none px-3 py-1 rounded-lg text-xs font-medium transition-colors ${
    active
      ? 'bg-accent text-white font-semibold shadow-sm'
      : 'bg-white/5 text-zinc-400 hover:text-white hover:bg-white/10'
  }`;

const SeasonPicker: React.FC<Pick<EpisodeSectionProps, 'seasons' | 'selectedSeason' | 'onSelectSeason'>> = ({
  seasons,
  selectedSeason,
  onSelectSeason,
}) => {
  if (seasons.length <= 1) return null;
  if (seasons.length > MAX_SEASON_TABS) {
    return (
      <select
        value={selectedSeason}
        onChange={(event) => onSelectSeason(Number(event.target.value))}
        aria-label="Season"
        className="px-3 py-1.5 rounded-lg bg-surface-elevated border border-white/10 text-xs text-white"
      >
        {seasons.map((season) => (
          <option key={season} value={season}>
            {seasonLabel(season)}
          </option>
        ))}
      </select>
    );
  }
  return (
    <div className="flex items-center gap-2 max-w-full overflow-x-auto no-scrollbar">
      {seasons.map((season) => (
        <button key={season} onClick={() => onSelectSeason(season)} className={pillClass(season === selectedSeason)}>
          {seasonLabel(season)}
        </button>
      ))}
    </div>
  );
};

const EpisodeCard: React.FC<{
  episode: Episode;
  fallbackPoster: string | null;
  onPlay: (e: Episode) => void;
  isHighlighted?: boolean;
}> = ({ episode, fallbackPoster, onPlay, isHighlighted }) => {
  const cardRef = React.useRef<HTMLDivElement | null>(null);

  React.useEffect(() => {
    if (isHighlighted && cardRef.current) {
      cardRef.current.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    }
  }, [isHighlighted]);

  const picture = episode.poster_url || fallbackPoster;
  return (
    <div
      ref={cardRef}
      onClick={() => onPlay(episode)}
      className={`group flex gap-3 sm:gap-4 p-3 rounded-2xl bg-surface-elevated/70 hover:bg-surface-elevated border cursor-pointer transition-all duration-200 ${
        isHighlighted
          ? 'border-accent ring-2 ring-accent/50 shadow-lg glow-accent'
          : 'border-white/5 hover:border-accent/40'
      }`}
    >
      <div className="relative w-28 sm:w-32 aspect-video rounded-xl overflow-hidden bg-midnight flex-shrink-0">
        {picture ? (
          <img
            src={picture}
            alt={episode.name}
            className="w-full h-full object-cover group-hover:scale-105 transition-transform duration-300"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center text-zinc-600">EP</div>
        )}
        <div className="absolute inset-0 bg-midnight/30 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
          <div className="w-8 h-8 rounded-full bg-accent text-white flex items-center justify-center shadow-lg">
            <Play className="w-3.5 h-3.5 fill-current ml-0.5" />
          </div>
        </div>
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-[11px] font-semibold text-accent mb-0.5">
          {episode.episode != null ? `Episode ${episode.episode}` : 'Episode'}
        </div>
        <h4 className="text-sm font-medium text-zinc-200 group-hover:text-white break-words">{episode.name}</h4>
        {episode.description && (
          <p className="text-xs text-zinc-500 line-clamp-2 mt-1 font-normal">{episode.description}</p>
        )}
      </div>
    </div>
  );
};

/** The season picker, sort button, episode ranges and the episodes of the selected season. */
export const EpisodeSection: React.FC<EpisodeSectionProps> = (props) => {
  const { seasons, selectedSeason, onSelectSeason, episodes, fallbackPoster, onPlay, highlightedKey } = props;
  const [ascending, setAscending] = React.useState(true);
  const [rangeIndex, setRangeIndex] = React.useState(0);
  const chunks = React.useMemo(() => chunkEpisodes(sortEpisodes(episodes, ascending)), [episodes, ascending]);
  const range = Math.min(rangeIndex, Math.max(chunks.length - 1, 0));

  React.useEffect(() => setRangeIndex(0), [selectedSeason, ascending]);

  React.useEffect(() => {
    if (!highlightedKey) return;
    const chunkIdx = chunks.findIndex((chunk) => chunk.some((ep) => ep.data === highlightedKey));
    if (chunkIdx >= 0) {
      setRangeIndex(chunkIdx);
    }
  }, [highlightedKey, chunks]);

  return (
    <div className="mt-10 sm:mt-14">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/10 pb-4 mb-4">
        <h2 className="text-xl font-bold text-white">
          Episodes <span className="text-sm font-normal text-zinc-500">({episodes.length})</span>
        </h2>
        <div className="flex items-center gap-2 max-w-full">
          <SeasonPicker seasons={seasons} selectedSeason={selectedSeason} onSelectSeason={onSelectSeason} />
          <button
            onClick={() => setAscending((value) => !value)}
            aria-label={ascending ? 'Showing oldest first' : 'Showing newest first'}
            title={ascending ? 'Oldest first' : 'Newest first'}
            className="p-2 rounded-lg bg-white/5 text-zinc-300 hover:text-white hover:bg-white/10 transition-colors"
          >
            {ascending ? <ArrowDownAZ className="w-4 h-4" /> : <ArrowUpAZ className="w-4 h-4" />}
          </button>
        </div>
      </div>

      {chunks.length > 1 && (
        <div className="flex gap-2 mb-5 overflow-x-auto no-scrollbar">
          {chunks.map((chunk, index) => (
            <button key={index} onClick={() => setRangeIndex(index)} className={pillClass(index === range)}>
              {chunkLabel(chunk, index)}
            </button>
          ))}
        </div>
      )}

      <div className="episode-grid">
        {(chunks[range] ?? []).map((episode) => (
          <EpisodeCard
            key={episode.id}
            episode={episode}
            fallbackPoster={fallbackPoster}
            onPlay={onPlay}
            isHighlighted={episode.data === highlightedKey}
          />
        ))}
      </div>
    </div>
  );
};
