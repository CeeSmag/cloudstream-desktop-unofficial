import { useEffect, useState } from 'react';
import { api } from '../services/api';

let answer: Promise<boolean> | null = null;

/**
 * True when mpv draws the picture into the application window itself (native video), so the
 * page must leave the player area see-through instead of painting frames on a canvas.
 */
export function useNativeVideo(): boolean {
  const [native, setNative] = useState(false);
  useEffect(() => {
    answer ??= api.getVideoMode().catch(() => false);
    let live = true;
    void answer.then((value) => {
      if (live) setNative(value);
    });
    return () => {
      live = false;
    };
  }, []);
  return native;
}
