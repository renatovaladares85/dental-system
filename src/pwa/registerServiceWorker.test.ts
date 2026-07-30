import { describe, expect, it } from 'vitest';

import { isApplicationServiceWorkerOrigin } from './registerServiceWorker';

describe('registro do shell PWA', () => {
  it('permite Service Worker somente na origem HTTPS da aplicação', () => {
    expect(isApplicationServiceWorkerOrigin({ protocol: 'https:', port: '8743' })).toBe(
      true,
    );
    expect(isApplicationServiceWorkerOrigin({ protocol: 'http:', port: '8742' })).toBe(
      false,
    );
    expect(isApplicationServiceWorkerOrigin({ protocol: 'https:', port: '8742' })).toBe(
      false,
    );
  });
});
