import { describe, expect, it } from 'vitest';
import { RepositoryEntry } from '../types';
import { sortRepositories } from './repositorySort';

const repo = (name: string, addedAtMs: number, url = `https://${name}/repo.json`): RepositoryEntry => ({
  name,
  url,
  addedAtMs,
  iconUrl: null,
});

describe('sortRepositories', () => {
  const list = [repo('Charlie', 100), repo('Alpha', 300), repo('Bravo', 200)];

  it('orders by name A to Z', () => {
    expect(sortRepositories(list, 'name').map((r) => r.name)).toEqual(['Alpha', 'Bravo', 'Charlie']);
  });

  it('orders by date newest first', () => {
    expect(sortRepositories(list, 'date').map((r) => r.name)).toEqual(['Alpha', 'Bravo', 'Charlie']);
  });

  it('does not mutate the input', () => {
    const input = [repo('B', 1), repo('A', 2)];
    sortRepositories(input, 'name');
    expect(input.map((r) => r.name)).toEqual(['B', 'A']);
  });
});
