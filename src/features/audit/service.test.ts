import { afterEach, describe, expect, it, vi } from 'vitest';

import { webAuditService } from './service';

afterEach(() => vi.unstubAllGlobals());

describe('serviço de auditoria', () => {
  it('limita a consulta e aceita eventos sanitizados', async () => {
    const event = {
      id: 'audit-1',
      actorType: 'USER',
      actorUserId: 'user-1',
      actorUsername: 'master.admin',
      action: 'USER_LOGGED_IN',
      entityType: 'session',
      entityId: 'session-1',
      result: 'SUCCESS',
      correlationId: '019b1234-1234-7123-8123-123456789abc',
      sessionId: 'session-1',
      source: 'LAN',
      occurredAt: '2026-07-31T12:00:00Z',
    };
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ events: [event] }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetchMock);

    await expect(webAuditService.listEvents(500)).resolves.toEqual([event]);
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/v1/audit/events?limit=100',
      expect.objectContaining({ credentials: 'same-origin' }),
    );
  });

  it('rejeita uma resposta fora do contrato', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ events: [{ password: 'segredo' }] }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    );

    await expect(webAuditService.listEvents()).rejects.toMatchObject({
      code: 'INVALID_SERVER_RESPONSE',
    });
  });
});
