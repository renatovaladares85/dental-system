import { apiRequest } from '../../lib/api';
import type { PairingCompletion, PairingMaterial, PairingService } from './types';

export const webPairingService: PairingService = {
  complete(token: string) {
    return apiRequest<PairingCompletion>('/pairing/complete', {
      method: 'POST',
      body: { token },
    });
  },
};

export function capturePairingMaterial(
  location = window.location,
  history = window.history,
): PairingMaterial | null {
  if (location.pathname !== '/pair') return null;

  const parameters = new URLSearchParams(location.hash.replace(/^#/, ''));
  const token = parameters.get('token') ?? '';
  const fingerprint = parameters.get('fingerprint') ?? '';

  history.replaceState(history.state, '', `${location.pathname}${location.search}`);

  if (!/^[A-Za-z0-9_-]{43}$/.test(token) || !/^[a-fA-F0-9]{64}$/.test(fingerprint)) {
    return null;
  }

  return {
    token,
    expectedFingerprintSha256: fingerprint.toLowerCase(),
  };
}
