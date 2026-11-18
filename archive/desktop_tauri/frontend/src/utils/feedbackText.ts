/** Longest text shown in a feedback pop-up before it is cut with an ellipsis. */
const MAX_POPUP_CHARS = 60;

/** `+5 s` or `−10 s` for a jump forward or back. */
export function seekText(deltaMs: number): string {
  const seconds = Math.round(Math.abs(deltaMs) / 1000);
  return `${deltaMs < 0 ? '−' : '+'}${seconds} s`;
}

/** `Volume 55%`, or `Muted` at zero. */
export function volumeText(percent: number): string {
  return percent <= 0 ? 'Muted' : `Volume ${Math.round(percent)}%`;
}

/** The pop-up for a chosen audio or subtitle track; `label` is null when subtitles were switched off. */
export function trackText(kind: 'audio' | 'subtitle', label: string | null): string {
  if (kind === 'subtitle') return label === null ? 'Subtitles off' : shorten(`Subtitles: ${label}`);
  return shorten(`Audio: ${label ?? 'default'}`);
}

/** The pop-up for a chosen stream source. */
export function sourceText(label: string): string {
  return shorten(`Source: ${label}`);
}

/** Cuts a text to the longest length a pop-up shows. */
export function shorten(text: string): string {
  return text.length <= MAX_POPUP_CHARS ? text : `${text.slice(0, MAX_POPUP_CHARS - 1)}…`;
}
