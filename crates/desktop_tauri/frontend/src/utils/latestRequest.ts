/**
 * Tickets for requests that replace one another (loading the Home page of whichever provider was
 * picked last). Only the newest ticket is still current; an answer that comes back on an older
 * ticket must be dropped, or a slow earlier request could overwrite the newer one.
 */
export interface LatestRequest {
  /** Starts a request; the returned function says whether it is still the newest. */
  begin: () => () => boolean;
}

export function createLatestRequest(): LatestRequest {
  let newest = 0;
  return {
    begin: () => {
      newest += 1;
      const mine = newest;
      return () => mine === newest;
    },
  };
}
