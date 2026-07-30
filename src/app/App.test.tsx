import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { AuthService, AuthenticatedSession } from '../features/auth/types';
import type { PairingCompletion, PairingService } from '../features/pairing/types';
import type { SetupService } from '../features/setup/types';
import { App } from './App';
import { HEALTH_HEARTBEAT_INTERVAL_MS } from './availability';
import type { AvailabilityService } from './availability';

const diagnostics = {
  sqlcipherVersion: '4.17.0',
  minimumDistributionVersion: '4.17.0',
  distributionReady: true,
  keyProtection: 'dpapi-current-user',
};

const authenticatedSession: AuthenticatedSession = {
  authenticated: true,
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

function setupService(): SetupService {
  return {
    isSetupOrigin: () => true,
    getStartupState: vi.fn(async () => ({ kind: 'ready' as const, diagnostics })),
    listStorageVolumes: vi.fn(async () => []),
    startInitialSetup: vi.fn(),
    resumeInitialSetup: vi.fn(),
    confirmInitialSetup: vi.fn(),
    startPairing: vi.fn(),
  };
}

function authService(overrides: Partial<AuthService> = {}): AuthService {
  return {
    getSession: vi.fn(async () => ({ authenticated: false as const })),
    login: vi.fn(async () => authenticatedSession),
    logout: vi.fn(async () => undefined),
    rotateCsrf: vi.fn(async () => 'csrf-2'),
    ...overrides,
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('inicialização da aplicação web', () => {
  it('autentica o administrador e permite encerrar a sessão', async () => {
    const user = userEvent.setup();
    const authentication = authService();
    render(
      <App context="application" service={setupService()} authService={authentication} />,
    );

    await user.type(await screen.findByLabelText('Nome de usuário'), 'marina.admin');
    await user.type(screen.getByLabelText('Senha'), 'uma frase longa e segura');
    await user.click(screen.getByRole('button', { name: 'Entrar' }));

    expect(await screen.findByText('Olá, Marina Costa')).toBeInTheDocument();
    expect(authentication.login).toHaveBeenCalledWith({
      username: 'marina.admin',
      password: 'uma frase longa e segura',
    });

    await user.click(screen.getByRole('button', { name: 'Encerrar sessão' }));
    await waitFor(() => expect(authentication.logout).toHaveBeenCalledTimes(1));
    expect(await screen.findByText('Acesse sua conta')).toBeInTheDocument();
  });

  it('mostra shell indisponível quando o servidor não responde', async () => {
    const authentication = authService({
      getSession: vi.fn(async () => {
        throw {
          code: 'SERVER_UNAVAILABLE',
          message: 'Servidor local indisponível.',
          correlationId: 'corr-network-1',
        };
      }),
    });
    render(
      <App context="application" service={setupService()} authService={authentication} />,
    );

    expect(await screen.findByText('Servidor indisponível')).toBeInTheDocument();
    expect(screen.getByText(/nenhuma operação ou dado clínico/)).toBeInTheDocument();
  });

  it('confirma a saúde local ao ficar offline e mantém o heartbeat sem reler a sessão', async () => {
    vi.useFakeTimers();
    let completeHealthCheck!: (healthy: boolean) => void;
    const firstHealthCheck = new Promise<boolean>((resolve) => {
      completeHealthCheck = resolve;
    });
    const checkHealth = vi
      .fn<AvailabilityService['checkHealth']>()
      .mockImplementationOnce(() => firstHealthCheck)
      .mockResolvedValue(true);
    const authentication = authService({
      getSession: vi.fn(async () => authenticatedSession),
    });

    render(
      <App
        context="application"
        service={setupService()}
        authService={authentication}
        availabilityService={{ checkHealth }}
      />,
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByText('Olá, Marina Costa')).toBeInTheDocument();

    act(() => {
      fireEvent(window, new Event('offline'));
      fireEvent(window, new Event('offline'));
    });
    expect(checkHealth).toHaveBeenCalledTimes(1);

    await act(async () => {
      completeHealthCheck(true);
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(screen.getByText('Olá, Marina Costa')).toBeInTheDocument();
    expect(screen.queryByText('Servidor indisponível')).not.toBeInTheDocument();
    expect(authentication.getSession).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(HEALTH_HEARTBEAT_INTERVAL_MS);
    });
    expect(checkHealth).toHaveBeenCalledTimes(2);
    expect(authentication.getSession).toHaveBeenCalledTimes(1);
  });

  it('mostra servidor indisponível quando o health check local falha', async () => {
    vi.useFakeTimers();
    const checkHealth = vi
      .fn<AvailabilityService['checkHealth']>()
      .mockResolvedValue(false);

    render(
      <App
        context="application"
        service={setupService()}
        authService={authService()}
        availabilityService={{ checkHealth }}
      />,
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByText('Acesse sua conta')).toBeInTheDocument();

    act(() => {
      fireEvent(window, new Event('offline'));
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(checkHealth).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Servidor indisponível')).toBeInTheDocument();
  });

  it('mantém pareamento público isolado de setup e autenticação', () => {
    const setup = setupService();
    const authentication = authService();
    const pendingPairing = new Promise<PairingCompletion>(() => undefined);
    const pairing: PairingService = {
      complete: vi.fn(() => pendingPairing),
    };
    const material = {
      token: 'A'.repeat(43),
      expectedFingerprintSha256: 'b'.repeat(64),
    };
    render(
      <App
        pairingMaterial={material}
        pairingService={pairing}
        service={setup}
        authService={authentication}
      />,
    );

    expect(screen.getByText('Confirme este dispositivo')).toBeInTheDocument();
    expect(pairing.complete).toHaveBeenCalledWith(material.token);
    expect(setup.getStartupState).not.toHaveBeenCalled();
    expect(authentication.getSession).not.toHaveBeenCalled();
  });

  it('exibe o controle local quando o setup está pronto', async () => {
    render(
      <App
        context="administration"
        service={setupService()}
        authService={authService()}
      />,
    );

    expect(await screen.findByText('Controle local do servidor')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Iniciar pareamento' })).toBeEnabled();
  });
});
