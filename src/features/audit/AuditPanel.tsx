import { ClipboardList, RefreshCw } from 'lucide-react';
import { useState } from 'react';

import { Alert } from '../../components/ui/Alert';
import { Button } from '../../components/ui/Button';
import { Card } from '../../components/ui/Card';
import { toApiError } from '../../lib/api';
import { webAuditService } from './service';
import type { AuditEvent, AuditService } from './types';

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat('pt-BR', {
        dateStyle: 'short',
        timeStyle: 'medium',
      }).format(date);
}

export function AuditPanel({ service = webAuditService }: { service?: AuditService }) {
  const [events, setEvents] = useState<AuditEvent[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ReturnType<typeof toApiError> | null>(null);

  async function load() {
    setBusy(true);
    setError(null);
    try {
      setEvents(await service.listEvents(50));
    } catch (caught) {
      setError(toApiError(caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className="mt-8 overflow-hidden">
      <div className="flex flex-wrap items-center justify-between gap-4 border-b border-slate-200 p-5">
        <div>
          <p className="flex items-center gap-2 font-bold text-slate-950">
            <ClipboardList className="size-5 text-petrol-700" aria-hidden="true" />
            Auditoria de segurança
          </p>
          <p className="mt-1 text-sm text-slate-600">
            Ações autoritativas registradas pelo servidor, sem senhas ou conteúdo clínico.
          </p>
        </div>
        <Button
          variant="secondary"
          busy={busy}
          icon={<RefreshCw className="size-4" aria-hidden="true" />}
          onClick={() => void load()}
        >
          {events ? 'Atualizar' : 'Consultar auditoria'}
        </Button>
      </div>

      {error ? (
        <Alert className="m-5" variant="danger" title="Falha ao consultar auditoria">
          <p>{error.message}</p>
          <p className="mt-1 text-xs">Referência: {error.correlationId}</p>
        </Alert>
      ) : null}

      {events ? (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[760px] text-left text-sm">
            <thead className="bg-slate-50 text-xs uppercase tracking-wide text-slate-500">
              <tr>
                <th className="px-5 py-3">Quando</th>
                <th className="px-5 py-3">Usuário</th>
                <th className="px-5 py-3">Ação</th>
                <th className="px-5 py-3">Entidade</th>
                <th className="px-5 py-3">Resultado</th>
                <th className="px-5 py-3">Referência</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {events.map((event) => (
                <tr key={event.id}>
                  <td className="whitespace-nowrap px-5 py-3 text-slate-600">
                    {formatDate(event.occurredAt)}
                  </td>
                  <td className="px-5 py-3 font-semibold text-slate-900">
                    {event.actorUsername ?? event.actorType}
                  </td>
                  <td className="px-5 py-3 font-mono text-xs text-slate-800">
                    {event.action}
                  </td>
                  <td className="px-5 py-3 text-slate-600">
                    {event.entityType}
                    {event.entityId ? ` · ${event.entityId}` : ''}
                  </td>
                  <td className="px-5 py-3 text-slate-700">{event.result}</td>
                  <td className="max-w-52 truncate px-5 py-3 font-mono text-xs text-slate-500">
                    {event.correlationId ?? '—'}
                  </td>
                </tr>
              ))}
              {events.length === 0 ? (
                <tr>
                  <td className="px-5 py-6 text-center text-slate-500" colSpan={6}>
                    Nenhum evento disponível.
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </div>
      ) : null}
    </Card>
  );
}
