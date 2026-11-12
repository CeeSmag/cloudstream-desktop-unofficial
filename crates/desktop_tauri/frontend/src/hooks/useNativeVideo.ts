import { useEffect, useState } from 'react';
import { api } from '../services/api';
import type { VideoMode } from '../types';

/**
 * True when mpv draws the picture into the application window itself (native video), so the
 * page must leave the player area see-through instead of painting frames on a canvas. The answer
 * is asked from the backend each time the player screen opens; until it arrives the canvas is assumed.
 */
export function useNativeVideo(): boolean {
  const [mode, setMode] = useState<VideoMode>('canvas');
  useEffect(() => {
    let live = true;
    api
      .getVideoMode()
      .then((answer) => {
        if (live) setMode(answer);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);
  return mode === 'native';
}
