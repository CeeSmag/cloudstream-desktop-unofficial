import { describe, expect, it } from 'vitest';
import { startupMessage, startupPercent } from './startupText';
import { StartupStatus } from '../types';

const status = (over: Partial<StartupStatus>): StartupStatus => ({
  ready: false,
  finished: 0,
  total: 0,
  current: '',
  failures: [],
  ...over,
});

describe('startupMessage', () => {
  it('says the runtime is starting while the list of extensions is unknown', () => {
    expect(startupMessage(status({}))).toBe('Starting the extension runtime…');
  });

  it('names the extension being loaded and counts from one', () => {
    expect(startupMessage(status({ total: 3, finished: 1, current: 'CineStream' }))).toBe(
      'Loading CineStream (2 of 3)',
    );
  });

  it('says it is finishing once every extension is done', () => {
    expect(startupMessage(status({ total: 3, finished: 3 }))).toBe('Finishing up…');
  });
});

describe('startupPercent', () => {
  it('is zero before the list is known and a share while loading', () => {
    expect(startupPercent(status({}))).toBe(0);
    expect(startupPercent(status({ total: 4, finished: 1 }))).toBe(25);
  });

  it('is full once ready', () => {
    expect(startupPercent(status({ ready: true }))).toBe(100);
  });
});
