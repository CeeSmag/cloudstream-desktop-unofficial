import { describe, expect, it } from 'vitest';
import { createClickSorter } from './clickSorter';

function rig() {
  const log: string[] = [];
  let paused = false;
  const sorter = createClickSorter(
    () => {
      paused = !paused;
      log.push(paused ? 'pause' : 'play');
    },
    () => log.push('fullscreen'),
  );
  return { log, sorter, paused: () => paused };
}

describe('createClickSorter', () => {
  it('a single click pauses at once, with nothing left waiting', () => {
    const { log, sorter } = rig();
    sorter.onClick(1);
    expect(log).toEqual(['pause']);
  });

  it('two separate clicks pause and play', () => {
    const { log, sorter } = rig();
    sorter.onClick(1);
    sorter.onClick(1);
    expect(log).toEqual(['pause', 'play']);
  });

  it('a double click goes fullscreen and leaves the video playing as it was', () => {
    const { log, sorter, paused } = rig();
    sorter.onClick(1);
    sorter.onClick(2);
    sorter.onDoubleClick();
    expect(log).toEqual(['pause', 'play', 'fullscreen']);
    expect(paused()).toBe(false);
  });

  it('a double click on a paused video leaves it paused', () => {
    const { sorter, paused } = rig();
    sorter.onClick(1);
    expect(paused()).toBe(true);
    sorter.onClick(1);
    sorter.onClick(2);
    sorter.onDoubleClick();
    expect(paused()).toBe(true);
  });

  it('a double click whose first click closed a menu only goes fullscreen', () => {
    const { log, sorter } = rig();
    sorter.onClick(1);
    sorter.onClick(0);
    sorter.onClick(2);
    sorter.onDoubleClick();
    expect(log).toEqual(['pause', 'fullscreen']);
  });

  it('a third click after a double click is a single click again', () => {
    const { log, sorter } = rig();
    sorter.onClick(1);
    sorter.onClick(2);
    sorter.onDoubleClick();
    sorter.onClick(1);
    expect(log).toEqual(['pause', 'play', 'fullscreen', 'pause']);
  });
});
