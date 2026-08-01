import { Clock3, LogOut, ShieldCheck, UserRound } from 'lucide-react';
import { useState } from 'react';

import { AppLogo } from '../../components/brand/AppLogo';
import { Alert } from '../../components/ui/Alert';
import { Button } from '../../components/ui/Button';
import { Card } from '../../components/ui/Card';
import { toApiError } from '../../lib/api';
import { AuditPanel } from '../audit/AuditPanel';
import type { AuthService, AuthenticatedSession } from './types';

function formatExpiration(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? 'horário indisponível'
    : new Intl.DateTimeFormat('pt-BR', {
        dateStyle: 'short',
        timeStyle: 'short',
      }).format(date);
}

interface AuthenticatedScreenProps {
  session: AuthenticatedSession;
  service: AuthService;
  onLoggedOut(): void;
  onServerUnavailable(): void;
}

export function AuthenticatedScreen({
  session,
  service,
  onLoggedOut,
  onServerUnavailable,
}: AuthenticatedScreenProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<ReturnType<typeof toApiError> | null>(null);

  async function logout() {
    setBusy(true);
    setError(null);
    try {
      await service.logout();
      onLoggedOut();
    } catch (caught) {
      const commandError = toApiError(caught);
      if (
        commandError.code === 'HTTP_401' ||
        commandError.code === 'SESSION_EXPIRED' ||
        commandError.code === 'AUTHENTICATION_REQUIRED'
      ) {
        onLoggedOut();
        return;
      }
      if (commandError.code === 'SERVER_UNAVAILABLE') {
        onServerUnavailable();
        return;
      }
      setError(commandError);
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="min-h-screen bg-slate-50">
      <header className="border-b border-slate-200 bg-white px-5 py-4 sm:px-8">
        <div className="mx-auto flex max-w-6xl items-center justify-between gap-4">
          <AppLogo compact />
          <Button
            variant="secondary"
            busy={busy}
            icon={<LogOut className="size-4" aria-hidden="true" />}
            onClick={() => void logout()}
          >
            Encerrar sessão
          </Button>
        </div>
      </header>

      <div className="mx-auto max-w-6xl px-5 py-10 sm:px-8">
        <p className="text-xs font-bold uppercase tracking-[0.16em] text-petrol-700">
          Sessão protegida
        </p>
        <h1 className="mt-2 text-3xl font-bold tracking-tight text-slate-950">
          Olá, {session.user.fullName}
        </h1>
        <p className="mt-3 max-w-2xl text-sm leading-6 text-slate-600">
          A autenticação está ativa. Os módulos clínicos serão adicionados nas próximas
          etapas sem alterar esta fronteira segura de sessão.
        </p>

        {error ? (
          <Alert className="mt-6" variant="danger" title="Falha ao encerrar a sessão">
            <p>{error.message}</p>
            <p className="mt-1 text-xs">Referência: {error.correlationId}</p>
          </Alert>
        ) : null}

        <div className="mt-8 grid gap-4 md:grid-cols-3">
          <Card className="p-5">
            <UserRound className="size-5 text-petrol-700" aria-hidden="true" />
            <p className="mt-4 text-xs font-semibold uppercase tracking-wide text-slate-500">
              Usuário
            </p>
            <p className="mt-1 font-bold text-slate-950">{session.user.username}</p>
            <p className="mt-1 text-sm text-slate-600">{session.user.email}</p>
          </Card>
          <Card className="p-5">
            <ShieldCheck className="size-5 text-petrol-700" aria-hidden="true" />
            <p className="mt-4 text-xs font-semibold uppercase tracking-wide text-slate-500">
              Perfis
            </p>
            <p className="mt-1 font-bold text-slate-950">
              {session.user.roles.join(', ') || 'Sem perfil'}
            </p>
          </Card>
          <Card className="p-5">
            <Clock3 className="size-5 text-petrol-700" aria-hidden="true" />
            <p className="mt-4 text-xs font-semibold uppercase tracking-wide text-slate-500">
              Expiração por inatividade
            </p>
            <p className="mt-1 font-bold text-slate-950">
              {formatExpiration(session.idleExpiresAt)}
            </p>
            <p className="mt-1 text-xs text-slate-500">
              Limite absoluto: {formatExpiration(session.absoluteExpiresAt)}
            </p>
          </Card>
        </div>
        {session.user.roles.includes('MASTER_ADMIN') ? <AuditPanel /> : null}
      </div>
    </main>
  );
}
