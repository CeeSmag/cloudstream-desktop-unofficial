import { describe, expect, it } from 'vitest';
import { keepsNativeMenu, menuItemsFor } from './contextMenu';

describe('context menu', () => {
  it('keeps the browser menu only in places where text is edited', () => {
    expect(keepsNativeMenu({ tagName: 'INPUT' })).toBe(true);
    expect(keepsNativeMenu({ tagName: 'textarea' })).toBe(true);
    expect(keepsNativeMenu({ tagName: 'DIV', isContentEditable: true })).toBe(true);
    expect(keepsNativeMenu({ tagName: 'DIV' })).toBe(false);
    expect(keepsNativeMenu({ tagName: 'IMG' })).toBe(false);
    expect(keepsNativeMenu({})).toBe(false);
    expect(keepsNativeMenu(null)).toBe(false);
  });

  it('has no entries of its own yet', () => {
    expect(menuItemsFor({ tagName: 'DIV' })).toEqual([]);
    expect(menuItemsFor(null)).toEqual([]);
  });
});
