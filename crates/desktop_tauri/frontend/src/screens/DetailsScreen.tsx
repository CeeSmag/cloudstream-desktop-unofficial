import React from 'react';
import { ArrowLeft, Play, BookmarkPlus, Check, Star, Calendar, Tag } from 'lucide-react';
import { Episode, MediaDetails } from '../types';

interface DetailsScreenProps {
  details: MediaDetails;
  onBack: () => void;
  onPlayEpisode: (ep: Episode) => void;
  isBookmarked: boolean;
  onToggleBookmark: () => void;
}

export const DetailsScreen: React.FC<DetailsScreenProps> = ({
  details,
  onBack,
  onPlayEpisode,
  isBookmarked,
  onToggleBookmark,
}) => {
  const [selectedSeason, setSelectedSeason] = React.useState<number>(1);

  // Group episodes by season
  const seasons = React.useMemo(() => {
    const map = new Map<number, Episode[]>();
    for (const ep of details.episodes) {
      const s = ep.season || 1;
      if (!map.has(s)) map.set(s, []);
      map.get(s)?.push(ep);
    }
    return map;
  }, [details.episodes]);

  const seasonNumbers = Array.from(seasons.keys()).sort((a, b) => a - b);
  const activeEpisodes = seasons.get(selectedSeason) || details.episodes;

  return (
    <div className="flex-1 overflow-y-auto pb-24 select-none">
      {/* Top Floating Back Bar */}
      <div className="sticky top-0 h-14 px-8 flex items-center z-30 bg-gradient-to-b from-midnight/90 to-transparent backdrop-blur-sm">
        <button
          onClick={onBack}
          className="flex items-center space-x-2 px-3 py-1.5 rounded-xl bg-white/10 hover:bg-white/20 text-white text-xs font-medium border border-white/10 transition-colors"
        >
          <ArrowLeft className="w-4 h-4" />
          <span>Back</span>
        </button>
      </div>

      {/* Hero Overview */}
      <div className="max-w-7xl mx-auto px-8 pt-2">
        <div className="flex flex-col md:flex-row items-start gap-8">
          {/* Vertical Poster */}
          <div className="w-56 flex-shrink-0 rounded-2xl overflow-hidden bg-surface-elevated border border-white/10 shadow-2xl">
            {details.poster_url ? (
              <img
                src={details.poster_url}
                alt={details.name}
                className="w-full h-auto aspect-[2/3] object-cover"
              />
            ) : (
              <div className="w-full aspect-[2/3] flex items-center justify-center bg-surface-elevated text-zinc-500">
                No Poster
              </div>
            )}
          </div>

          {/* Details Column */}
          <div className="flex-1">
            <div className="flex items-center space-x-2.5 mb-2.5">
              {details.media_type && (
                <span className="px-2.5 py-0.5 rounded-md bg-accent/20 text-accent font-semibold text-xs border border-accent/30 uppercase tracking-wide">
                  {details.media_type}
                </span>
              )}
              <span className="px-2.5 py-0.5 rounded-md bg-white/10 text-zinc-300 font-medium text-xs border border-white/10">
                {details.provider}
              </span>
              {details.year && (
                <span className="flex items-center space-x-1 text-zinc-400 text-xs">
                  <Calendar className="w-3.5 h-3.5" />
                  <span>{details.year}</span>
                </span>
              )}
              <span className="flex items-center space-x-1 text-gold text-xs font-medium">
                <Star className="w-3.5 h-3.5 fill-gold" />
                <span>8.6</span>
              </span>
            </div>

            <h1 className="text-3xl md:text-4xl font-extrabold text-white tracking-tight mb-4">
              {details.name}
            </h1>

            {details.plot && (
              <p className="text-zinc-300 text-sm leading-relaxed max-w-3xl mb-6">
                {details.plot}
              </p>
            )}

            {/* Tag Pills */}
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

            {/* Main Action Buttons */}
            <div className="flex items-center space-x-3.5">
              {details.episodes.length === 0 && details.data_url && (
                <button
                  onClick={() =>
                    onPlayEpisode({
                      id: details.data_url ?? '',
                      data: details.data_url ?? '',
                      name: details.name,
                      season: null,
                      episode: null,
                      poster_url: details.poster_url,
                      description: null,
                    })
                  }
                  className="flex items-center space-x-2.5 px-6 py-3 rounded-xl bg-accent hover:bg-accent-hover text-white text-sm font-semibold transition-all duration-200 shadow-lg glow-accent"
                >
                  <Play className="w-4 h-4 fill-current" />
                  <span>Play</span>
                </button>
              )}

              {details.episodes.length > 0 && (
                <button
                  onClick={() => {
                    const first = details.episodes[0];
                    if (first) onPlayEpisode(first);
                  }}
                  className="flex items-center space-x-2.5 px-6 py-3 rounded-xl bg-accent hover:bg-accent-hover text-white text-sm font-semibold transition-all duration-200 shadow-lg glow-accent"
                >
                  <Play className="w-4 h-4 fill-current" />
                  <span>Play Episode 1</span>
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
                {isBookmarked ? (
                  <>
                    <Check className="w-4 h-4 text-emerald-400" />
                    <span>Saved in Library</span>
                  </>
                ) : (
                  <>
                    <BookmarkPlus className="w-4 h-4" />
                    <span>Save to Library</span>
                  </>
                )}
              </button>
            </div>
          </div>
        </div>

        {/* Episodes Section */}
        {details.episodes.length > 0 && (
          <div className="mt-14">
            <div className="flex items-center justify-between border-b border-white/10 pb-4 mb-6">
              <h2 className="text-xl font-bold text-white">Episodes</h2>

              {/* Season Selector Tabs */}
              {seasonNumbers.length > 1 && (
                <div className="flex items-center space-x-2">
                  {seasonNumbers.map((sn) => (
                    <button
                      key={sn}
                      onClick={() => setSelectedSeason(sn)}
                      className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors ${
                        selectedSeason === sn
                          ? 'bg-accent text-white font-semibold shadow-sm'
                          : 'bg-white/5 text-zinc-400 hover:text-white hover:bg-white/10'
                      }`}
                    >
                      Season {sn}
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Episode Grid */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {activeEpisodes.map((ep) => (
                <div
                  key={ep.id}
                  onClick={() => onPlayEpisode(ep)}
                  className="group flex space-x-4 p-3 rounded-2xl bg-surface-elevated/70 hover:bg-surface-elevated border border-white/5 hover:border-accent/40 cursor-pointer transition-all duration-200"
                >
                  {/* Thumbnail */}
                  <div className="relative w-32 aspect-video rounded-xl overflow-hidden bg-midnight flex-shrink-0">
                    {ep.poster_url || details.poster_url ? (
                      <img
                        src={ep.poster_url || details.poster_url || ''}
                        alt={ep.name}
                        className="w-full h-full object-cover group-hover:scale-105 transition-transform duration-300"
                      />
                    ) : (
                      <div className="w-full h-full flex items-center justify-center text-zinc-600">
                        EP
                      </div>
                    )}
                    <div className="absolute inset-0 bg-midnight/30 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity">
                      <div className="w-8 h-8 rounded-full bg-accent text-white flex items-center justify-center shadow-lg">
                        <Play className="w-3.5 h-3.5 fill-current ml-0.5" />
                      </div>
                    </div>
                  </div>

                  {/* Info */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center space-x-2 text-[11px] font-semibold text-accent mb-0.5">
                      <span>EP {ep.episode || 1}</span>
                      {ep.season && <span className="text-zinc-500">• S{ep.season}</span>}
                    </div>
                    <h4 className="text-sm font-medium text-zinc-200 group-hover:text-white truncate">
                      {ep.name}
                    </h4>
                    {ep.description && (
                      <p className="text-xs text-zinc-500 line-clamp-2 mt-1 font-normal">
                        {ep.description}
                      </p>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
