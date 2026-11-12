import React from 'react';
import { SourceFilters } from '../hooks/useSourceFilters';
import { LanguageFlag } from '../utils/subtitleFlags';
import { ResolutionFilter } from './ResolutionFilter';
import { SubtitleFilter } from './SubtitleFilter';

interface SourceMenuHeaderProps {
  filters: SourceFilters;
  /** The viewer's preferred subtitle languages, shown as toggles. */
  languages: LanguageFlag[];
  /** How many sources there are before any filter. */
  total: number;
}

/** The filters above the list of sources: resolution, then subtitle languages. */
export const SourceMenuHeader: React.FC<SourceMenuHeaderProps> = ({ filters, languages, total }) => (
  <>
    <ResolutionFilter
      toggled={filters.resolutionFilter}
      counts={filters.tierCounts}
      onToggle={filters.toggleResolution}
      shown={filters.resolutionCount}
      total={total}
    />
    <SubtitleFilter
      languages={languages}
      toggled={filters.subtitleFilter}
      onToggle={filters.toggleSubtitle}
      shown={filters.subtitleCount}
      total={total}
    />
  </>
);
