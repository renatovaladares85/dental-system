import { afterEach, describe, expect, it, vi } from 'vitest';

import { capturePairingMaterial, webPairingService } from './service';

afterEach(() => vi.unstubAllGlobals());

describe('adapter de pareamento', () => {
  it('captura token/fingerprint do fragmento e o remove da barra de endereço', () => {
    const replaceState = vi.fn();
    const token = 'A'.repeat(43);
    const fingerprint = 'ab'.repeat(32);
    const location = {
      pathname: '/pair',
      search: '?source=qr',
      hash: `#token=${token}&fingerprint=${fingerprint}`,
    } as Location;
    const history = { state: null, replaceState } as unknown as History;

    expect(capturePairingMaterial(location, history)).toEqual({
      token,
      expectedFingerprintSha256: fingerprint,
    });
    expect(replaceState).toHaveBeenCalledWith(null, '', '/pair?source=qr');
  });

  it('recusa fragmentos fora do formato antes de chamar a API', () => {
    const location = {
      pathname: '/pair',
      search: '',
      hash: '#token=curto&fingerprint=1234',
    } as Location;
    const history = { state: null, replaceState: vi.fn() } as unknown as History;

    expect(capturePairingMaterial(location, history)).toBeNull();
  });

  it('envia token somente no corpo do POST one-shot', async () => {
    const fetchMock = vi.fn<typeof fetch>(
      async () =>
        new Response(
          JSON.stringify({
            fingerprintSha256: 'ab'.repeat(32),
            caCertificateDerBase64: 'AQID',
            caFileName: 'dental-ca.cer',
            serverUrl: 'https://dental.local:8743/',
          }),
          { status: 200, headers: { 'Content-Type': 'application/json' } },
        ),
    );
    vi.stubGlobal('fetch', fetchMock);
    const token = 'A'.repeat(43);

    await webPairingService.complete(token);

    const [url, request] = fetchMock.mock.calls[0]!;
    expect(url).toBe('/api/v1/pairing/complete');
    expect(url).not.toContain(token);
    expect(JSON.parse((request as RequestInit).body as string)).toEqual({ token });
  });
});
