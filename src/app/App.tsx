import { AlertTriangle, RefreshCw, ShieldCheck, WifiOff } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';

import { AppLogo } from '../components/brand/AppLogo';
import { Alert } from '../components/ui/Alert';
import { Button } from '../components/ui/Button';
import { Card } from '../components/ui/Card';
import { AuthenticatedScreen } from '../features/auth/AuthenticatedScreen';
import { LoginScreen } from '../features/auth/LoginScreen';
import { webAuthService } from '../features/auth/service';
import type { AuthService, AuthSession } from '../features/auth/types';
import { HostControlScreen } from '../features/pairing/HostControlScreen';
import { PairingClientScreen } from '../features/pairing/PairingClientScreen';
import { capturePairingMaterial, webPairingService } from '../features/pairing/service';
import type { PairingMaterial, PairingService } from '../features/pairing/types';
import { RecoveryRequiredScreen } from '../features/recovery/RecoveryRequiredScreen';
import { SetupWizard } from '../features/setup/SetupWizard';
import { toCommandError, webSetupService } from '../features/setup/service';
import type { CommandError, SetupService, StartupState } from '../features/setup/types';
import { HEALTH_HEARTBEAT_INTERVAL_MS, webAvailabilityService } from './availability';
import type { AvailabilityService } from './availability';

interface AppProps {
  service?: SetupService;
  authService?: AuthService;
  pairingService?: PairingService;
  availabilityService?: AvailabilityService;
  context?: 'administration' | 'application';
  pairingMaterial?: PairingMaterial | null;
}

type LoadedView =
  { kind: 'setup'; state: StartupState } | { kind: 'auth'; session: AuthSession };

type ApplicationState =
  | { status: 'loading' }
  | { status: 'error'; error: CommandError }
  | { status: 'loaded'; view: LoadedView };

function serverUnavailableError(): CommandError {
  return {
    code: 'SERVER_UNAVAILABLE',
    message:
      'O servidor local não está disponível. Verifique se o computador servidor está ligado e conectado à rede da clínica.',
    correlationId: crypto.randomUUID(),
  };
}

function LoadingScreen() {
  return (
    <main className="grid min-h-screen place-items-center bg-slate-50 px-5">
      <div className="text-center" role="status">
        <span className="mx-auto grid size-14 animate-pulse place-items-center rounded-xl bg-petrol-700 text-white">
          <ShieldCheck className="size-7" aria-hidden="true" />
        </span>
        <p className="mt-4 text-sm font-semibold text-slate-700">
          Conectando ao servidor local…
        </p>
      </div>
    </main>
  );
}

function UnavailableScreen({ error, onRetry }: { error: CommandError; onRetry(): void }) {
  const unavailable = error.code === 'SERVER_UNAVAILABLE';

  return (
    <main className="flex min-h-screen items-center justify-center bg-slate-50 px-5 py-10">
      <Card className="w-full max-w-lg p-7 text-center">
        <div className="mb-7 flex justify-center">
          <AppLogo />
        </div>
        <span className="mx-auto grid size-12 place-items-center rounded-xl bg-red-50 text-red-700">
          {unavailable ? (
            <WifiOff className="size-6" aria-hidden="true" />
          ) : (
            <AlertTriangle className="size-6" aria-hidden="true" />
          )}
        </span>
        <h1 className="mt-5 text-2xl font-bold text-slate-950">
          {unavailable ? 'Servidor indisponível' : 'Falha ao carregar o sistema'}
        </h1>
        <p className="mt-3 text-sm leading-6 text-slate-600">
          {unavailable
            ? 'A interface permanece aberta, mas nenhuma operação ou dado clínico fica disponível sem a conexão local.'
            : 'A aplicação falhou de forma segura antes de liberar qualquer operação.'}
        </p>
        <Alert className="mt-5 text-left" variant="danger">
          <p>{error.message}</p>
          <p className="mt-1 text-xs">Referência: {error.correlationId}</p>
        </Alert>
        <Button
          className="mt-6"
          icon={<RefreshCw className="size-4" aria-hidden="true" />}
          onClick={onRetry}
        >
          Tentar novamente
        </Button>
      </Card>
    </main>
  );
}

export function App({
  service = webSetupService,
  authService = webAuthService,
  pairingService = webPairingService,
  availabilityService = webAvailabilityService,
  context,
  pairingMaterial,
}: AppProps) {
  const [capturedPairing] = useState<PairingMaterial | null>(() =>
    pairingMaterial === undefined ? capturePairingMaterial() : pairingMaterial,
  );
  const pairingRoute =
    pairingMaterial !== undefined || window.location.pathname === '/pair';
  const mode = context ?? (service.isSetupOrigin() ? 'administration' : 'application');
  const [reload, setReload] = useState(0);
  const [application, setApplication] = useState<ApplicationState>({
    status: 'loading',
  });

  const load = useCallback(async () => {
    setApplication({ status: 'loading' });
    try {
      if (mode === 'administration') {
        setApplication({
          status: 'loaded',
          view: { kind: 'setup', state: await service.getStartupState() },
        });
      } else {
        setApplication({
          status: 'loaded',
          view: { kind: 'auth', session: await authService.getSession() },
        });
      }
    } catch (error) {
      setApplication({ status: 'error', error: toCommandError(error) });
    }
  }, [authService, mode, service]);

  useEffect(() => {
    if (pairingRoute) return;
    let active = true;
    queueMicrotask(() => {
      if (active) void load();
    });
    return () => {
      active = false;
    };
  }, [load, pairingRoute, reload]);

  useEffect(() => {
    function retryWhenOnline() {
      setReload((value) => value + 1);
    }

    window.addEventListener('online', retryWhenOnline);
    return () => {
      window.removeEventListener('online', retryWhenOnline);
    };
  }, []);

  const shouldMonitorAvailability =
    mode === 'application' &&
    !pairingRoute &&
    application.status === 'loaded' &&
    application.view.kind === 'auth';

  useEffect(() => {
    if (!shouldMonitorAvailability) return;

    let active = true;
    let checking = false;
    let timer: number | undefined;

    function scheduleNextCheck() {
      if (!active || timer !== undefined) return;
      timer = window.setTimeout(() => {
        timer = undefined;
        void checkNow();
      }, HEALTH_HEARTBEAT_INTERVAL_MS);
    }

    async function checkNow() {
      if (!active || checking) return;
      checking = true;
      if (timer !== undefined) {
        window.clearTimeout(timer);
        timer = undefined;
      }

      let healthy: boolean;
      try {
        healthy = await availabilityService.checkHealth();
      } catch {
        healthy = false;
      } finally {
        checking = false;
      }

      if (!active) return;
      if (!healthy) {
        setApplication({ status: 'error', error: serverUnavailableError() });
        return;
      }
      scheduleNextCheck();
    }

    function checkWhenBrowserReportsOffline() {
      void checkNow();
    }

    scheduleNextCheck();
    window.addEventListener('offline', checkWhenBrowserReportsOffline);

    return () => {
      active = false;
      if (timer !== undefined) window.clearTimeout(timer);
      window.removeEventListener('offline', checkWhenBrowserReportsOffline);
    };
  }, [availabilityService, shouldMonitorAvailability]);

  const activeSession =
    application.status === 'loaded' &&
    application.view.kind === 'auth' &&
    application.view.session.authenticated
      ? application.view.session
      : null;

  useEffect(() => {
    if (!activeSession) return;
    let active = true;

    async function revalidateSession() {
      try {
        const session = await authService.getSession();
        if (active) {
          setApplication({ status: 'loaded', view: { kind: 'auth', session } });
        }
      } catch (caught) {
        if (active) {
          setApplication({ status: 'error', error: toCommandError(caught) });
        }
      }
    }

    function revalidateWhenVisible() {
      if (document.visibilityState === 'visible') void revalidateSession();
    }

    const remaining = new Date(activeSession.idleExpiresAt).getTime() - Date.now();
    const timeout = window.setTimeout(
      () => void revalidateSession(),
      Math.max(0, Math.min(remaining, 2_147_483_647)),
    );
    document.addEventListener('visibilitychange', revalidateWhenVisible);

    return () => {
      active = false;
      window.clearTimeout(timeout);
      document.removeEventListener('visibilitychange', revalidateWhenVisible);
    };
  }, [activeSession, authService]);

  if (pairingRoute) {
    return <PairingClientScreen material={capturedPairing} service={pairingService} />;
  }
  if (application.status === 'loading') return <LoadingScreen />;
  if (application.status === 'error') {
    return (
      <UnavailableScreen
        error={application.error}
        onRetry={() => setReload((value) => value + 1)}
      />
    );
  }

  if (application.view.kind === 'auth') {
    const session = application.view.session;

    if (session.authenticated) {
      return (
        <AuthenticatedScreen
          session={session}
          service={authService}
          onLoggedOut={() =>
            setApplication({
              status: 'loaded',
              view: { kind: 'auth', session: { authenticated: false } },
            })
          }
          onServerUnavailable={() =>
            setApplication({ status: 'error', error: serverUnavailableError() })
          }
        />
      );
    }

    return (
      <LoginScreen
        service={authService}
        onAuthenticated={(authenticated) =>
          setApplication({
            status: 'loaded',
            view: { kind: 'auth', session: authenticated },
          })
        }
        onServerUnavailable={() =>
          setApplication({ status: 'error', error: serverUnavailableError() })
        }
      />
    );
  }

  const state = application.view.state;

  if (state.kind === 'ready') {
    return <HostControlScreen service={service} diagnostics={state.diagnostics} />;
  }

  if (state.kind === 'recovery_required') {
    return <RecoveryRequiredScreen reasonCode={state.reasonCode} />;
  }

  return (
    <SetupWizard
      service={service}
      initialState={state}
      onReady={(readyState) =>
        setApplication({
          status: 'loaded',
          view: { kind: 'setup', state: readyState },
        })
      }
    />
  );
}
