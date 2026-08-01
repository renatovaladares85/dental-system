import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { AuditPanel } from './AuditPanel';
import type { AuditService } from './types';

describe('painel de auditoria', () => {
  it('carrega ações apenas sob solicitação e identifica usuário e resultado', async () => {
    const service: AuditService = {
      listEvents: vi.fn().mockResolvedValue([
        {
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
        },
      ]),
    };
    render(<AuditPanel service={service} />);

    expect(service.listEvents).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Consultar auditoria' }));

    expect(await screen.findByText('master.admin')).toBeInTheDocument();
    expect(screen.getByText('USER_LOGGED_IN')).toBeInTheDocument();
    expect(screen.getByText('SUCCESS')).toBeInTheDocument();
    expect(service.listEvents).toHaveBeenCalledWith(50);
  });

  it('mostra referência sanitizada quando a consulta falha', async () => {
    const service: AuditService = {
      listEvents: vi.fn().mockRejectedValue({
        code: 'DATABASE_UNAVAILABLE',
        message: 'Não foi possível consultar a auditoria.',
        correlationId: '019b1234-1234-7123-8123-123456789abc',
      }),
    };
    render(<AuditPanel service={service} />);

    await userEvent.click(screen.getByRole('button', { name: 'Consultar auditoria' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Não foi possível consultar a auditoria.',
    );
    expect(screen.getByRole('alert')).toHaveTextContent(
      '019b1234-1234-7123-8123-123456789abc',
    );
  });
});
