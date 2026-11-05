import React from 'react';
import { tileInitial } from '../../utils/extensionText';

interface IconTileProps {
  /** Address of the icon image; null or an image that fails to load shows the first letter of the name. */
  url: string | null;
  name: string;
  size?: 'sm' | 'md' | 'lg';
}

const SIZES = { sm: 'w-8 h-8 text-xs', md: 'w-11 h-11 text-sm', lg: 'w-14 h-14 text-lg' };

/** The icon of a repository or extension, with a lettered tile when it has none or it cannot be loaded. */
export const IconTile: React.FC<IconTileProps> = React.memo(({ url, name, size = 'md' }) => {
  const [failed, setFailed] = React.useState(false);

  // A different address is a different image: try it again.
  React.useEffect(() => setFailed(false), [url]);

  const box = `${SIZES[size]} flex-none rounded-xl overflow-hidden`;
  if (!url || failed) {
    return (
      <span className={`${box} flex items-center justify-center bg-accent/15 text-accent font-bold`} aria-hidden="true">
        {tileInitial(name)}
      </span>
    );
  }
  return (
    <img
      src={url}
      alt=""
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      onError={() => setFailed(true)}
      className={`${box} object-cover bg-white/5`}
    />
  );
});
