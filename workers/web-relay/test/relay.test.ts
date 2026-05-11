import { describe, expect, it } from 'vitest';

describe('relay room validation', () => {
  it('documents accepted room shape', () => {
    expect(/^[a-zA-Z0-9_-]{6,64}$/.test('auralux_123')).toBe(true);
    expect(/^[a-zA-Z0-9_-]{6,64}$/.test('../bad')).toBe(false);
  });
});

