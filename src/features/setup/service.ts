import { apiRequest, toApiError } from '../../lib/api';
import {
  parseSetupHttpResponse,
  setupProgressSchema,
  startupStateSchema,
  storageVolumesResponseSchema,
} from './httpSchemas';
import type {
  ConfirmSetupInput,
  InitialSetupInput,
  PairingSession,
  SetupService,
  StorageInput,
} from './types';

function isLoopbackHost(hostname: string): boolean {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '::1';
}

export function isAdministrativeOrigin(location = window.location): boolean {
  return (
    isLoopbackHost(location.hostname) &&
    (location.port === '8742' ||
      (import.meta.env.DEV &&
        import.meta.env.VITE_APP_CONTEXT !== 'application' &&
        location.port === '1420'))
  );
}

export const toCommandError = toApiError;

export const webSetupService: SetupService = {
  isSetupOrigin: isAdministrativeOrigin,

  async getStartupState() {
    const response = await apiRequest<unknown>('/setup/state');
    return parseSetupHttpResponse(startupStateSchema, response);
  },

  async listStorageVolumes() {
    const response = await apiRequest<unknown>('/setup/storage-volumes');
    return parseSetupHttpResponse(storageVolumesResponseSchema, response).volumes;
  },

  async startInitialSetup(input: InitialSetupInput) {
    const response = await apiRequest<unknown>('/setup/start', {
      method: 'POST',
      body: input,
      timeoutMs: 180_000,
    });
    return parseSetupHttpResponse(setupProgressSchema, response);
  },

  async resumeInitialSetup(setupId: string, storage: StorageInput) {
    const response = await apiRequest<unknown>(
      `/setup/${encodeURIComponent(setupId)}/resume`,
      {
        method: 'POST',
        body: { storage },
        timeoutMs: 180_000,
      },
    );
    return parseSetupHttpResponse(setupProgressSchema, response);
  },

  async confirmInitialSetup(input: ConfirmSetupInput) {
    const { setupId, ...confirmation } = input;
    const response = await apiRequest<unknown>(
      `/setup/${encodeURIComponent(setupId)}/confirm`,
      {
        method: 'POST',
        body: confirmation,
        timeoutMs: 180_000,
      },
    );
    return parseSetupHttpResponse(startupStateSchema, response);
  },

  startPairing() {
    return apiRequest<PairingSession>('/pairing/start', { method: 'POST' });
  },
};
