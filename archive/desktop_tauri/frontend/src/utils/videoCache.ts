/** "256 MB" below a gigabyte, "1 GB" / "1.5 GB" from there on. */
export function sizeLabel(mb: number): string {
  if (mb < 1024) return `${mb} MB`;
  const gb = mb / 1024;
  return `${Number.isInteger(gb) ? gb : gb.toFixed(1)} GB`;
}

/** How a size splits by the ahead share: [ahead, behind] in MB, as the backend splits it. */
export function splitMb(sizeMb: number, aheadPercent: number): [number, number] {
  const ahead = Math.floor((sizeMb * aheadPercent) / 100);
  return [ahead, sizeMb - ahead];
}

/** "1.5 GB ahead, 512 MB behind" for the size while a video plays. */
export function splitText(sizeMb: number, aheadPercent: number): string {
  const [ahead, behind] = splitMb(sizeMb, aheadPercent);
  return `${sizeLabel(ahead)} ahead, ${sizeLabel(behind)} behind`;
}

/** The same for a video that waits after the viewer left the player: half of each part. */
export function awayText(sizeMb: number, aheadPercent: number): string {
  const [ahead, behind] = splitMb(sizeMb, aheadPercent);
  return `${sizeLabel(Math.floor(ahead / 2))} ahead, ${sizeLabel(Math.floor(behind / 2))} behind`;
}
