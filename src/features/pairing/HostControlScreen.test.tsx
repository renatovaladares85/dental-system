import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { SetupService } from '../setup/types';
import { HostControlScreen } from './HostControlScreen';

const diagnostics = {
  sqlcipherVersion: '4.17.0',
  minimumDistributionVersion: '4.17.0',
  distributionReady: true,
  keyProtection: 'dpapi-current-user',
};

function service(expiresAt: string): SetupService {
  const token = 'A'.repeat(43);
  const fingerprintSha256 = 'ab'.repeat(32);
  return {
    isSetupOrigin: () => true,
    getStartupState: vi.fn(),
    listStorageVolumes: vi.fn(async () => []),
    startInitialSetup: vi.fn(),
    resumeInitialSetup: vi.fn(),
    confirmInitialSetup: vi.fn(),
    startPairing: vi.fn(async () => ({
      token,
      pairingUrl: `https://dental-123.local:8743/pair#token=${token}&fingerprint=${fingerprintSha256}`,
      fingerprintSha256,
      expiresAt,
    })),
  };
}

afterEach(() => vi.useRealTimers());

describe('controle de pareamento', () => {
  it('gera QR local e o remove ao fechar', async () => {
    const user = userEvent.setup();
    render(
      <HostControlScreen
        service={service(new Date(Date.now() + 60_000).toISOString())}
        diagnostics={diagnostics}
      />,
    );

    await user.click(screen.getByRole('button', { name: 'Iniciar pareamento' }));
    expect(await screen.findByLabelText('QR code do pareamento')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Ocultar QR code' }));
    expect(screen.queryByLabelText('QR code do pareamento')).not.toBeInTheDocument();
  });

  it('remove QR e token ao expirar sem persistir o material', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-22T12:00:00Z'));
    render(
      <HostControlScreen
        service={service('2026-07-22T12:00:01Z')}
        diagnostics={diagnostics}
      />,
    );

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Iniciar pareamento' }));
    });
    expect(screen.getByLabelText('QR code do pareamento')).toBeInTheDocument();

    act(() => vi.advanceTimersByTime(1_001));
    expect(screen.queryByLabelText('QR code do pareamento')).not.toBeInTheDocument();
    expect(screen.queryByText('A'.repeat(43))).not.toBeInTheDocument();
    expect(localStorage).toHaveLength(0);
    expect(sessionStorage).toHaveLength(0);
  });

  it('não abre a LAN nem o pareamento quando o gate de distribuição falha', () => {
    render(
      <HostControlScreen
        service={service(new Date(Date.now() + 60_000).toISOString())}
        diagnostics={{ ...diagnostics, distributionReady: false }}
      />,
    );

    expect(
      screen.queryByRole('link', { name: 'Abrir o sistema' }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Iniciar pareamento' })).toBeDisabled();
  });
});
