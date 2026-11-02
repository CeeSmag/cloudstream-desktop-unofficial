import React from 'react';
import { api } from '../services/api';

/** Bytes before the pixels: width u32, height u32, serial u64 (little-endian). */
const HEADER_BYTES = 16;
/** Pause between polls when the player has no newer frame. */
const IDLE_POLL_MS = 8;

interface VideoCanvasProps {
  /** False for audio-only streams; the canvas is hidden so no stale picture stays on screen. */
  visible: boolean;
}

/** Draws mpv's frames and keeps the player informed about the size of the picture area. */
export const VideoCanvas: React.FC<VideoCanvasProps> = ({ visible }) => {
  const canvasRef = React.useRef<HTMLCanvasElement>(null);

  // Tell the player how many physical pixels the picture area has.
  React.useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const report = () => {
      const ratio = window.devicePixelRatio || 1;
      const width = Math.round(canvas.clientWidth * ratio);
      const height = Math.round(canvas.clientHeight * ratio);
      if (width > 0 && height > 0) api.setVideoSurface(width, height).catch(() => undefined);
    };
    report();
    const observer = new ResizeObserver(report);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  // Pull frames as fast as the player produces them.
  React.useEffect(() => {
    let stopped = false;
    let lastSerial = 0;
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return undefined;

    const pump = async () => {
      while (!stopped) {
        try {
          const body = await api.getVideoFrame(lastSerial);
          if (body.byteLength > HEADER_BYTES) {
            const header = new DataView(body);
            const width = header.getUint32(0, true);
            const height = header.getUint32(4, true);
            lastSerial = Number(header.getBigUint64(8, true));
            if (canvas.width !== width || canvas.height !== height) {
              canvas.width = width;
              canvas.height = height;
            }
            const pixels = new Uint8ClampedArray(body, HEADER_BYTES, width * height * 4);
            context.putImageData(new ImageData(pixels, width, height), 0, 0);
            await new Promise<void>((next) => requestAnimationFrame(() => next()));
          } else {
            await new Promise((next) => setTimeout(next, IDLE_POLL_MS));
          }
        } catch {
          // The backend is busy or the player just restarted; try again shortly.
          await new Promise((next) => setTimeout(next, 100));
        }
      }
    };
    void pump();
    return () => {
      stopped = true;
    };
  }, []);

  return (
    <canvas
      ref={canvasRef}
      className="pointer-events-none absolute inset-0 -z-10 h-full w-full object-contain bg-black"
      style={{ display: visible ? 'block' : 'none' }}
    />
  );
};
