import { describe, expect, it } from 'vitest';
import { escapeHtml } from './html';

describe('escapeHtml', () => {
  it('escapes markup characters so they render as text', () => {
    expect(escapeHtml('<img src=x onerror=alert(1)>')).toBe(
      '&lt;img src=x onerror=alert(1)&gt;',
    );
  });

  it('escapes quotes and ampersands', () => {
    expect(escapeHtml(`a & "b" 'c'`)).toBe('a &amp; &quot;b&quot; &#39;c&#39;');
  });

  it('passes plain values through and stringifies numbers', () => {
    expect(escapeHtml('cell-42')).toBe('cell-42');
    expect(escapeHtml(7)).toBe('7');
  });
});
