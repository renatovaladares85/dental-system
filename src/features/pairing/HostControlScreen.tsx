import { Copy, ExternalLink, Link2, MonitorSmartphone, RefreshCw, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { QRCodeSVG } from 'qrcode.react';

import { AppLogo } from '../../components/brand/AppLogo';
import { Alert } from '../../components/ui/Alert';
import { Button } from '../../components/ui/Button';
import { Card } from '../../components/ui/Card';
import { toApiError } from '../../lib/api';
import type { PairingSession, SecurityDiagnostics, SetupService } from '../setup/types';

function applicationUrl(): string {
  return 'https://localhost:8743/';
}

function formatFingerprint(value: string): string {
  return (
    value
      .replace(/[^a-fA-F0-9]/g, '')
      .toUpperCase()
      .match(/.{1,4}/g)
      ?.join(' ') ?? value
  );
}

function formatExpiration(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat('pt-BR', {
        dateStyle: 'short',
        timeStyle: 'medium',
      }).format(date);
}

function validatePairingSession(value: PairingSession): PairingSession {
  try {
    const url = new URL(value.pairingUrl);
    const parameters = new URLSearchParams(url.hash.replace(/^#/, ''));
    const fingerprint = value.fingerprintSha256.toLowerCase();
    if (
      url.protocol !== 'https:' ||
      url.pathname !== '/pair' ||
      url.search !== '' ||
      url.username !== '' ||
      url.password !== '' ||
      !/^[A-Za-z0-9_-]{43}$/.test(value.token) ||
      !/^[a-f0-9]{64}$/.test(fingerprint) ||
      parameters.get('token') !== value.token ||
      parameters.get('fingerprint')?.toLowerCase() !== fingerprint
    ) {
      throw new Error();
    }
    return value;
  } catch {
    throw {
      code: 'INVALID_PAIRING_RESPONSE',
      message: 'O servidor retornou uma autorização de pareamento inválida.',
      correlationId: crypto.randomUUID(),
    };
  }
}

export function HostControlScreen({
  service,
  diagnostics,
}: {
  service: SetupService;
  diagnostics: SecurityDiagnostics;
}) {
  const [pairing, setPairing] = useState<PairingSession | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<ReturnType<typeof toApiError> | null>(null);

  useEffect(() => {
    if (!pairing) return;
    const remaining = new Date(pairing.expiresAt).getTime() - Date.now();
    const timeout = window.setTimeout(
      () => setPairing(null),
      Number.isFinite(remaining) ? Math.max(0, remaining) : 0,
    );
    return () => window.clearTimeout(timeout);
  }, [pairing]);

  async function startPairing() {
    setBusy(true);
    setError(null);
    setCopied(false);
    try {
      const created = validatePairingSession(await service.startPairing());
      const expiresAt = new Date(created.expiresAt).getTime();
      if (!Number.isFinite(expiresAt) || expiresAt <= Date.now()) {
        throw {
          code: 'INVALID_PAIRING_EXPIRATION',
          message: 'O servidor retornou uma autorização de pareamento expirada.',
          correlationId: crypto.randomUUID(),
        };
      }
      setPairing(created);
    } catch (caught) {
      setError(toApiError(caught));
    } finally {
      setBusy(false);
    }
  }

  async function copyPairingUrl() {
    if (!pairing) return;
    try {
      if (!navigator.clipboard) throw new Error();
      await navigator.clipboard.writeText(pairing.pairingUrl);
      setCopied(true);
    } catch {
      setError({
        code: 'CLIPBOARD_UNAVAILABLE',
        message: 'Não foi possível copiar. Selecione o endereço manualmente.',
        correlationId: crypto.randomUUID(),
      });
    }
  }

  return (
    <main className="min-h-screen bg-slate-50 px-5 py-8 sm:px-8">
      <div className="mx-auto max-w-4xl">
        <header className="flex flex-col gap-5 sm:flex-row sm:items-center sm:justify-between">
          <AppLogo />
          {diagnostics.distributionReady ? (
            <a
              className="inline-flex min-h-11 items-center justify-center gap-2 rounded-lg bg-petrol-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-petrol-800 focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-petrol-500"
              href={applicationUrl()}
            >
              Abrir o sistema
              <ExternalLink className="size-4" aria-hidden="true" />
            </a>
          ) : null}
        </header>

        <div className="mt-10">
          <p className="text-xs font-bold uppercase tracking-[0.16em] text-emerald-700">
            {diagnostics.distributionReady
              ? 'Servidor pronto'
              : 'Configuração concluída · LAN bloqueada'}
          </p>
          <h1 className="mt-2 text-3xl font-bold tracking-tight text-slate-950">
            Controle local do servidor
          </h1>
          <p className="mt-3 max-w-2xl text-sm leading-6 text-slate-600">
            Use esta tela somente neste computador para abrir o sistema ou autorizar um
            novo dispositivo da rede da clínica.
          </p>
        </div>

        {!diagnostics.distributionReady ? (
          <Alert className="mt-6" variant="warning" title="Build de desenvolvimento">
            SQLCipher {diagnostics.sqlcipherVersion ?? 'não detectado'}. A distribuição
            exige {diagnostics.minimumDistributionVersion} ou superior.
          </Alert>
        ) : null}

        <Card className="mt-6 p-6 sm:p-8">
          <div className="flex flex-col gap-5 sm:flex-row sm:items-start sm:justify-between">
            <div className="flex gap-4">
              <span className="grid size-11 shrink-0 place-items-center rounded-xl bg-petrol-50 text-petrol-700">
                <MonitorSmartphone className="size-5" aria-hidden="true" />
              </span>
              <div>
                <h2 className="font-bold text-slate-950">Parear outro dispositivo</h2>
                <p className="mt-1 max-w-xl text-sm leading-6 text-slate-600">
                  A autorização dura dez minutos e pode ser usada uma única vez. Compare a
                  impressão digital antes de confiar no certificado.
                </p>
              </div>
            </div>
            <Button
              className="shrink-0"
              busy={busy}
              disabled={!diagnostics.distributionReady}
              icon={
                pairing ? (
                  <RefreshCw className="size-4" aria-hidden="true" />
                ) : (
                  <Link2 className="size-4" aria-hidden="true" />
                )
              }
              onClick={() => void startPairing()}
            >
              {pairing ? 'Gerar novo acesso' : 'Iniciar pareamento'}
            </Button>
          </div>

          {error ? (
            <Alert className="mt-5" variant="danger" title="Pareamento indisponível">
              <p>{error.message}</p>
              <p className="mt-1 text-xs">Referência: {error.correlationId}</p>
            </Alert>
          ) : null}

          {pairing ? (
            <div className="mt-6 grid gap-4" aria-live="polite">
              <div className="grid gap-5 rounded-lg bg-slate-100 p-4 sm:grid-cols-[auto_1fr] sm:items-center">
                <div
                  className="mx-auto rounded-lg bg-white p-3 shadow-sm"
                  aria-label="QR code do pareamento"
                >
                  <QRCodeSVG
                    value={pairing.pairingUrl}
                    size={168}
                    level="M"
                    marginSize={1}
                    title="Endereço temporário de pareamento"
                  />
                </div>
                <div className="min-w-0">
                  <div className="flex items-start justify-between gap-3">
                    <p className="text-xs font-semibold uppercase tracking-wide text-slate-500">
                      Endereço temporário
                    </p>
                    <button
                      type="button"
                      className="rounded-md p-1 text-slate-500 hover:bg-white hover:text-slate-900 focus-visible:outline-3 focus-visible:outline-petrol-500"
                      aria-label="Ocultar QR code"
                      onClick={() => setPairing(null)}
                    >
                      <X className="size-4" aria-hidden="true" />
                    </button>
                  </div>
                  <code className="mt-2 block break-all text-sm text-slate-900">
                    {pairing.pairingUrl}
                  </code>
                  <p className="mt-3 text-xs leading-5 text-slate-600">
                    Ocultar esta tela não revoga o token; ele permanece válido até o
                    horário indicado ou até a geração de um novo acesso.
                  </p>
                  <Button
                    className="mt-3"
                    variant="secondary"
                    icon={<Copy className="size-4" aria-hidden="true" />}
                    onClick={() => void copyPairingUrl()}
                  >
                    {copied ? 'Copiado' : 'Copiar endereço'}
                  </Button>
                </div>
              </div>
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="rounded-lg border border-slate-200 p-4">
                  <p className="text-xs font-semibold uppercase tracking-wide text-slate-500">
                    Token de uso único
                  </p>
                  <code className="mt-2 block break-all text-sm font-semibold text-slate-900">
                    {pairing.token}
                  </code>
                </div>
                <div className="rounded-lg border border-slate-200 p-4">
                  <p className="text-xs font-semibold uppercase tracking-wide text-slate-500">
                    Válido até
                  </p>
                  <p className="mt-2 text-sm font-semibold text-slate-900">
                    {formatExpiration(pairing.expiresAt)}
                  </p>
                </div>
              </div>
              <div className="rounded-lg border border-amber-200 bg-amber-50 p-4">
                <p className="text-xs font-semibold uppercase tracking-wide text-amber-800">
                  Fingerprint SHA-256
                </p>
                <code className="mt-2 block break-all text-xs leading-6 text-amber-950">
                  {formatFingerprint(pairing.fingerprintSha256)}
                </code>
              </div>
            </div>
          ) : null}
        </Card>
      </div>
    </main>
  );
}
