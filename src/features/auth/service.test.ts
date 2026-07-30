import { afterEach, describe, expect, it, vi } from 'vitest';

import { createWebAuthService } from './service';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(status === 204 ? null : JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': 'csrf-1' },
  });
}

const authenticatedSession = {
  authenticated: true as const,
  user: {
    id: 'user-1',
    fullName: 'Marina Costa',
    username: 'marina.admin',
    email: 'marina@example.test',
    roles: ['MASTER_ADMIN'],
  },
  idleExpiresAt: '2099-07-22T15:30:00Z',
  absoluteExpiresAt: '2099-07-23T03:00:00Z',
};

afterEach(() => vi.unstubAllGlobals());

describe('sessão HTTP', () => {
  it('mantém o token CSRF apenas em memória e o envia no logout', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(authenticatedSession))
      .mockResolvedValueOnce(jsonResponse(null, 204));
    vi.stubGlobal('fetch', fetchMock);
    const service = createWebAuthService();

    await service.login({ username: 'marina.admin', password: 'uma frase segura' });
    await service.logout();

    const loginRequest = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(JSON.parse(loginRequest.body as string)).toEqual({
      username: 'marina.admin',
      password: 'uma frase segura',
    });
    const logoutRequest = fetchMock.mock.calls[1]?.[1] as RequestInit;
    expect(new Headers(logoutRequest.headers).get('X-CSRF-Token')).toBe('csrf-1');
    expect(localStorage).toHaveLength(0);
  });

  it('restaura sessão e rotaciona CSRF sem localStorage', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(authenticatedSession))
      .mockResolvedValueOnce(
        new Response(null, {
          status: 204,
          headers: { 'X-CSRF-Token': 'csrf-2' },
        }),
      );
    vi.stubGlobal('fetch', fetchMock);
    const service = createWebAuthService();

    await expect(service.getSession()).resolves.toEqual(authenticatedSession);
    await expect(service.rotateCsrf()).resolves.toBe('csrf-2');

    const rotateRequest = fetchMock.mock.calls[1]?.[1] as RequestInit;
    expect(new Headers(rotateRequest.headers).get('X-CSRF-Token')).toBe('csrf-1');
    expect(localStorage).toHaveLength(0);
  });

  it('aceita sessão anônima sem CSRF e falha fechada se faltar CSRF autenticado', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ authenticated: false }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify(authenticatedSession), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    vi.stubGlobal('fetch', fetchMock);
    const service = createWebAuthService();

    await expect(service.getSession()).resolves.toEqual({ authenticated: false });
    await expect(service.getSession()).rejects.toMatchObject({
      code: 'INVALID_SERVER_RESPONSE',
    });
  });
});
