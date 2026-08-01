export interface AuditEvent {
  id: string;
  actorType: string;
  actorUserId: string | null;
  actorUsername: string | null;
  action: string;
  entityType: string;
  entityId: string | null;
  result: 'SUCCESS' | 'DENIED' | 'FAILURE';
  correlationId: string | null;
  sessionId: string | null;
  source: string;
  occurredAt: string;
}

export interface AuditService {
  listEvents(limit?: number): Promise<AuditEvent[]>;
}
