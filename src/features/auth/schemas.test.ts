import { describe, expect, it } from 'vitest';

import { loginSchema } from './schemas';

describe('schema de login', () => {
  it('limita a senha por code points Unicode, não por unidades UTF-16', () => {
    expect(
      loginSchema.safeParse({ username: 'marina.admin', password: '🌱'.repeat(128) })
        .success,
    ).toBe(true);
    expect(
      loginSchema.safeParse({ username: 'marina.admin', password: '🌱'.repeat(129) })
        .success,
    ).toBe(false);
  });
});
