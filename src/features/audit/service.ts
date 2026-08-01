import { z } from 'zod';

import { apiRequest } from '../../lib/api';
import type { AuditEvent, AuditService } from './types';

const auditEventSchema = z.object({
  id: z.string().min(1),
  actorType: z.string().min(1),
  actorUserId: z.string().nullable(),
  actorUsername: z.string().nullable(),
  action: z.string().min(1),
  entityType: z.string().min(1),
  entityId: z.string().nullable(),
  result: z.enum(['SUCCESS', 'DENIED', 'FAILURE']),
  correlationId: z.string().nullable(),
  sessionId: z.string().nullable(),
  source: z.string().min(1),
  occurredAt: z.string().min(1),
});

const responseSchema = z.object({ events: z.array(auditEventSchema).max(100) });

export const webAuditService: AuditService = {
  async listEvents(limit = 50): Promise<AuditEvent[]> {
    const bounded = Math.max(1, Math.min(100, Math.trunc(limit)));
    const response = responseSchema.safeParse(
      await apiRequest<unknown>(`/audit/events?limit=${bounded}`),
    );
    if (!response.success) {
      throw {
        code: 'INVALID_SERVER_RESPONSE',
        message: 'O servidor retornou uma auditoria inválida.',
        correlationId: crypto.randomUUID(),
      };
    }
    return response.data.events;
  },
};
