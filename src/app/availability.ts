import { apiRequest } from '../lib/api';

export const HEALTH_HEARTBEAT_INTERVAL_MS = 20_000;
const HEALTH_REQUEST_TIMEOUT_MS = 3_000;

export interface AvailabilityService {
  checkHealth(): Promise<boolean>;
}

function isHealthyResponse(value: unknown): boolean {
  return (
    typeof value === 'object' &&
    value !== null &&
    'status' in value &&
    (value as Record<string, unknown>).status === 'ok'
  );
}

export const webAvailabilityService: AvailabilityService = {
  async checkHealth() {
    try {
      return isHealthyResponse(
        await apiRequest<unknown>('/health', { timeoutMs: HEALTH_REQUEST_TIMEOUT_MS }),
      );
    } catch {
      return false;
    }
  },
};
