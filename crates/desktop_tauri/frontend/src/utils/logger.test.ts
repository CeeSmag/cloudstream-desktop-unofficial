import { describe, expect, it } from 'vitest';
import { describeValue, isDebugLogging, setDebugLogging, shouldSend } from './logger';

describe('shouldSend', () => {
  it('sends only warnings and errors while debug logging is off', () => {
    expect(shouldSend('error', false)).toBe(true);
    expect(shouldSend('warn', false)).toBe(true);
    expect(shouldSend('info', false)).toBe(false);
    expect(shouldSend('debug', false)).toBe(false);
    expect(shouldSend('trace', false)).toBe(false);
  });

  it('sends everything while debug logging is on', () => {
    expect(shouldSend('debug', true)).toBe(true);
    expect(shouldSend('trace', true)).toBe(true);
  });
});

describe('debug logging switch', () => {
  it('starts off and follows the backend', () => {
    expect(isDebugLogging()).toBe(false);
    setDebugLogging(true);
    expect(isDebugLogging()).toBe(true);
    setDebugLogging(false);
    expect(isDebugLogging()).toBe(false);
  });
});

describe('describeValue', () => {
  it('keeps strings, shows errors with their stack and objects as JSON', () => {
    expect(describeValue('plain')).toBe('plain');
    expect(describeValue(new Error('boom'))).toContain('boom');
    expect(describeValue({ a: 1 })).toBe('{"a":1}');
  });
});
