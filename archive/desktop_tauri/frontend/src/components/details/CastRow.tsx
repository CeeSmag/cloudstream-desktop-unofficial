import React from 'react';
import { CastMember } from '../../types';

/** The cast of a title as a row of round photos with names, scrolling sideways. */
export const CastRow: React.FC<{ cast: CastMember[] }> = ({ cast }) => {
  if (cast.length === 0) return null;
  return (
    <div className="mt-10 sm:mt-14">
      <h2 className="text-xl font-bold text-white border-b border-white/10 pb-4 mb-5">Cast</h2>
      <div className="flex gap-4 overflow-x-auto no-scrollbar pb-2">
        {cast.map((member, index) => (
          <div key={`${member.name}-${index}`} className="flex-none w-24 text-center">
            {member.image ? (
              <img
                src={member.image}
                alt={member.name}
                loading="lazy"
                decoding="async"
                className="w-20 h-20 mx-auto rounded-full object-cover bg-surface-elevated border border-white/10"
              />
            ) : (
              <div className="w-20 h-20 mx-auto rounded-full bg-surface-elevated border border-white/10 flex items-center justify-center text-2xl text-zinc-500">
                {member.name.charAt(0)}
              </div>
            )}
            <p className="mt-2 text-xs font-medium text-zinc-200 break-words">{member.name}</p>
            {member.role && <p className="text-[11px] text-zinc-500">{member.role}</p>}
          </div>
        ))}
      </div>
    </div>
  );
};
