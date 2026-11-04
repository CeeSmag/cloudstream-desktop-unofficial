import { describe, expect, it } from 'vitest';
import { createPromptWindow, promptKey, skipButtonVisible, WINDOW_MS, WindowTimers } from './skipWindow';

/** Timers that only fire when the test moves the clock. */
function fakeTimers() {
  let time = 0;
  let nextId = 1;
  const pending = new Map<number, { at: number; action: () => void }>();
  const timers: WindowTimers = {
    setTimer: (action, ms) => {
      const id = nextId++;
      pending.set(id, { at: time + ms, action });
      return id;
    },
    clearTimer: (handle) => {
      pending.delete(handle as number);
    },
  };
  const advance = (ms: number) => {
    const end = time + ms;
    for (;;) {
      const due = [...pending.entries()].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      time = due[1].at;
      pending.delete(due[0]);
      due[1].action();
    }
    time = end;
  };
  return { timers, advance, waiting: () => pending.size };
}

function rig() {
  const changes: boolean[] = [];
  const clock = fakeTimers();
  const window = createPromptWindow((open) => changes.push(open), clock.timers);
  return { changes, window, ...clock };
}

describe('promptKey', () => {
  it('names a skip by where it ends and the next episode button as next', () => {
    expect(promptKey({ label: 'Skip Intro', action: 'seek', toMs: 60_000 })).toBe('seek:60000');
    expect(promptKey({ label: 'Next episode', action: 'next' })).toBe('next');
    expect(promptKey(null)).toBeNull();
  });

  it('tells two parts of the video apart', () => {
    const a = promptKey({ label: 'Skip Intro', action: 'seek', toMs: 60_000 });
    const b = promptKey({ label: 'Skip Credits', action: 'seek', toMs: 1_400_000 });
    expect(a).not.toBe(b);
  });
});

describe('skipButtonVisible', () => {
  it('shows during the first seconds, or while the controls show, and never without a prompt', () => {
    const base = { hasPrompt: true, windowOpen: false, controlsVisible: false };
    expect(skipButtonVisible(base)).toBe(false);
    expect(skipButtonVisible({ ...base, windowOpen: true })).toBe(true);
    expect(skipButtonVisible({ ...base, controlsVisible: true })).toBe(true);
    expect(skipButtonVisible({ hasPrompt: false, windowOpen: true, controlsVisible: true })).toBe(false);
  });
});

describe('createPromptWindow', () => {
  it('opens when a prompt appears and closes exactly after five seconds', () => {
    const { changes, window, advance } = rig();
    window.show('seek:60000');
    expect(changes).toEqual([true]);
    advance(WINDOW_MS - 1);
    expect(changes).toEqual([true]);
    advance(1);
    expect(changes).toEqual([true, false]);
  });

  it('does not restart the five seconds when the same prompt is told again', () => {
    const { changes, window, advance } = rig();
    window.show('seek:60000');
    advance(3000);
    window.show('seek:60000');
    advance(2000);
    expect(changes).toEqual([true, false]);
  });

  it('closes at once when the prompt goes away and opens a fresh window when it comes back', () => {
    const { changes, window, advance, waiting } = rig();
    window.show('seek:60000');
    advance(1000);
    window.show(null);
    expect(changes).toEqual([true, false]);
    expect(waiting()).toBe(0);
    window.show('seek:60000');
    expect(changes).toEqual([true, false, true]);
    advance(WINDOW_MS);
    expect(changes).toEqual([true, false, true, false]);
  });

  it('gives a different prompt its own five seconds', () => {
    const { changes, window, advance } = rig();
    window.show('seek:60000');
    advance(4000);
    window.show('next');
    advance(4000);
    expect(changes).toEqual([true, true]);
    advance(1000);
    expect(changes).toEqual([true, true, false]);
  });

  it('does nothing for no prompt at the start, and stop cancels a running window', () => {
    const { changes, window, advance, waiting } = rig();
    window.show(null);
    expect(changes).toEqual([]);
    window.show('next');
    window.stop();
    expect(waiting()).toBe(0);
    advance(WINDOW_MS * 2);
    expect(changes).toEqual([true]);
  });
});
