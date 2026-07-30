import { apiRequestWithResponse } from '../../lib/api';
import type { AuthService, AuthSession, LoginInput } from './types';
import { authSessionSchema } from './schemas';

function csrfFrom(response: Response): string {
  const csrfToken = response.headers.get('X-CSRF-Token');
  if (csrfToken) return csrfToken;

  throw {
    code: 'INVALID_SERVER_RESPONSE',
    message: 'O servidor não forneceu a proteção necessária para a sessão.',
    correlationId: response.headers.get('x-correlation-id') ?? crypto.randomUUID(),
  };
}

function validSession(value: unknown): AuthSession {
  const result = authSessionSchema.safeParse(value);
  if (result.success) return result.data;

  throw {
    code: 'INVALID_SERVER_RESPONSE',
    message: 'O servidor retornou uma sessão inválida.',
    correlationId: crypto.randomUUID(),
  };
}

export function createWebAuthService(): AuthService {
  let csrfToken: string | null = null;

  return {
    async getSession() {
      const { data, response } = await apiRequestWithResponse<unknown>('/auth/session');
      const session = validSession(data);
      csrfToken = session.authenticated ? csrfFrom(response) : null;
      return session;
    },

    async login(input: LoginInput) {
      const { data, response } = await apiRequestWithResponse<unknown>('/auth/login', {
        method: 'POST',
        body: input,
      });
      const session = validSession(data);
      if (!session.authenticated) {
        throw {
          code: 'INVALID_SERVER_RESPONSE',
          message: 'O servidor não confirmou a autenticação.',
          correlationId: crypto.randomUUID(),
        };
      }
      csrfToken = csrfFrom(response);
      return session;
    },

    async logout() {
      await apiRequestWithResponse<void>('/auth/logout', {
        method: 'POST',
        ...(csrfToken ? { csrfToken } : {}),
      });
      csrfToken = null;
    },

    async rotateCsrf() {
      const { response } = await apiRequestWithResponse<void>('/auth/csrf/rotate', {
        method: 'POST',
        ...(csrfToken ? { csrfToken } : {}),
      });
      csrfToken = csrfFrom(response);
      return csrfToken;
    },
  };
}

export const webAuthService = createWebAuthService();
