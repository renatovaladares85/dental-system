import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { PairingService } from './types';
import { PairingClientScreen } from './PairingClientScreen';

const certificate = Uint8Array.from([48, 3, 2, 1, 5]);
const certificateBase64 = btoa(String.fromCharCode(...certificate));
const fingerprint = '417c7763c4e320a6b747b3cb0c6d22f93741b29a32b48594b8eb4c144fe6d729';
const material = {
  token: 'A'.repeat(43),
  expectedFingerprintSha256: fingerprint,
};

function service(responseFingerprint = fingerprint): PairingService {
  return {
    complete: vi.fn(async () => ({
      fingerprintSha256: responseFingerprint,
      caCertificateDerBase64: certificateBase64,
      caFileName: 'dental-local-ca.cer',
      serverUrl: 'https://dental.local:8743/',
    })),
  };
}

afterEach(() => vi.restoreAllMocks());

describe('cliente de pareamento', () => {
  it('só libera o certificado depois de verificar os três fingerprints', async () => {
    const createObjectUrl = vi.fn(() => 'blob:dental-ca');
    const revokeObjectUrl = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      value: createObjectUrl,
    });
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      value: revokeObjectUrl,
    });

    const { unmount } = render(
      <PairingClientScreen material={material} service={service()} />,
    );

    const download = await screen.findByRole('link', {
      name: 'Baixar certificado .cer',
    });
    expect(download).toHaveAttribute('href', 'blob:dental-ca');
    expect(download).toHaveAttribute('download', 'dental-local-ca.cer');
    expect(createObjectUrl).toHaveBeenCalledTimes(1);

    unmount();
    expect(revokeObjectUrl).toHaveBeenCalledWith('blob:dental-ca');
  });

  it('falha fechada se resposta e fragmento divergirem', async () => {
    const createObjectUrl = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      value: createObjectUrl,
    });

    render(<PairingClientScreen material={material} service={service('c'.repeat(64))} />);

    expect(await screen.findByText('Pareamento recusado')).toBeInTheDocument();
    expect(
      screen.getByText(/fingerprint do certificado não corresponde/),
    ).toBeInTheDocument();
    expect(createObjectUrl).not.toHaveBeenCalled();
  });
});
