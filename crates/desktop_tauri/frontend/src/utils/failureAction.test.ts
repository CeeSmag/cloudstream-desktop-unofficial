import { describe, expect, it } from 'vitest';
import { failureAction } from './failureAction';

describe('failureAction', () => {
  it('asks to verify while a site waits for the go-ahead', () => {
    expect(failureAction({ host: 'animepahe.pw', provider: 'AnimePahe' })).toBe('ask-to-verify');
  });

  it('shows the error otherwise', () => {
    expect(failureAction(null)).toBe('show-error');
  });
});
