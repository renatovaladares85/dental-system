export interface ApiError {
  code: string;
  message: string;
  correlationId: string;
  fieldErrors?: Record<string, string>;
}

interface RequestOptions {
  method?: 'GET' | 'POST';
  body?: unknown;
  csrfToken?: string;
  timeoutMs?: number;
}

const API_PREFIX = '/api/v1';

function hasStringProperty(
  value: unknown,
  property: string,
): value is Record<string, unknown> {
  return (
    typeof value === 'object' &&
    value !== null &&
    property in value &&
    typeof (value as Record<string, unknown>)[property] === 'string'
  );
}

function normalizeFieldErrors(value: unknown): Record<string, string> | undefined {
  if (Array.isArray(value)) {
    const entries = value.flatMap((item) =>
      hasStringProperty(item, 'field') && hasStringProperty(item, 'message')
        ? [[item.field as string, item.message as string] as const]
        : [],
    );

    return entries.length > 0 ? Object.fromEntries(entries) : undefined;
  }

  if (typeof value !== 'object' || value === null) return undefined;

  const entries = Object.entries(value).flatMap(([field, message]) =>
    typeof message === 'string' ? ([[field, message]] as const) : [],
  );

  return entries.length > 0 ? Object.fromEntries(entries) : undefined;
}

export function toApiError(error: unknown): ApiError {
  if (
    hasStringProperty(error, 'code') &&
    hasStringProperty(error, 'message') &&
    hasStringProperty(error, 'correlationId')
  ) {
    const candidate = error as Record<string, unknown>;
    const fieldErrors = normalizeFieldErrors(candidate.fieldErrors);

    return {
      code: candidate.code as string,
      message: candidate.message as string,
      correlationId: candidate.correlationId as string,
      ...(fieldErrors ? { fieldErrors } : {}),
    };
  }

  return {
    code: 'UNEXPECTED_ERROR',
    message: 'Não foi possível concluir a operação. Tente novamente.',
    correlationId: crypto.randomUUID(),
  };
}

function unavailableError(): ApiError {
  return {
    code: 'SERVER_UNAVAILABLE',
    message:
      'O servidor local não está disponível. Verifique se o computador servidor está ligado e conectado à rede da clínica.',
    correlationId: crypto.randomUUID(),
  };
}

async function readError(response: Response): Promise<ApiError> {
  try {
    return toApiError(await response.json());
  } catch {
    return {
      code: `HTTP_${response.status}`,
      message:
        response.status === 401
          ? 'Sua sessão não é válida ou expirou.'
          : 'O servidor recusou a operação de forma segura.',
      correlationId: response.headers.get('x-correlation-id') ?? crypto.randomUUID(),
    };
  }
}

export async function apiRequest<T>(
  path: string,
  { method = 'GET', body, csrfToken, timeoutMs }: RequestOptions = {},
): Promise<T> {
  return (
    await apiRequestWithResponse<T>(path, {
      method,
      ...(body !== undefined ? { body } : {}),
      ...(csrfToken ? { csrfToken } : {}),
      ...(timeoutMs !== undefined ? { timeoutMs } : {}),
    })
  ).data;
}

export async function apiRequestWithResponse<T>(
  path: string,
  { method = 'GET', body, csrfToken, timeoutMs = 15_000 }: RequestOptions = {},
): Promise<{ data: T; response: Response }> {
  const headers = new Headers({ Accept: 'application/json' });
  if (body !== undefined) headers.set('Content-Type', 'application/json');
  if (csrfToken) headers.set('X-CSRF-Token', csrfToken);

  let response: Response;
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    response = await fetch(`${API_PREFIX}${path}`, {
      method,
      headers,
      credentials: 'same-origin',
      cache: 'no-store',
      signal: controller.signal,
      ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
    });
  } catch {
    throw unavailableError();
  } finally {
    window.clearTimeout(timeout);
  }

  if (!response.ok) throw await readError(response);
  if (response.status === 204) return { data: undefined as T, response };

  try {
    return { data: (await response.json()) as T, response };
  } catch {
    throw {
      code: 'INVALID_SERVER_RESPONSE',
      message: 'O servidor retornou uma resposta inválida.',
      correlationId: response.headers.get('x-correlation-id') ?? crypto.randomUUID(),
    } satisfies ApiError;
  }
}
