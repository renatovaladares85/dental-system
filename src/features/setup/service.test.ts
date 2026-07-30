import { afterEach, describe, expect, it, vi } from 'vitest';

import { apiRequest } from '../../lib/api';
import { webSetupService, toCommandError } from './service';

const SETUP_ID = '018f0f7d-82ab-7d6e-b234-0123456789ab';
const BACKUP_VOLUME_ID = `volume_${'A'.repeat(22)}`;
const RECOVERY_VOLUME_ID = `volume_${'B'.repeat(22)}`;
const SHA256 = 'ab'.repeat(32);
const RECOVERY_CODE = 'A'.repeat(48);

const diagnostics = {
  sqlcipherVersion: '4.17.0',
  minimumDistributionVersion: '4.17.0',
  distributionReady: true,
  keyProtection: 'dpapi-current-user',
};

function validVolume() {
  return {
    id: BACKUP_VOLUME_ID,
    rootPath: 'E:\\',
    label: 'Backup',
    fileSystem: 'NTFS',
    availableBytes: 42_000_000,
    kind: 'removable' as const,
    writable: true,
    destinationPath: 'E:\\OfflineDentalSystem\\Artifacts',
  };
}

function validProgress() {
  return {
    setupId: SETUP_ID,
    stage: 'verification' as const,
    completedStages: [
      'database',
      'master_user',
      'recovery_package',
      'initial_backup',
      'verification',
    ] as const,
    artifacts: {
      recoveryPackage: {
        path: 'F:\\OfflineDentalSystem\\Artifacts\\Recovery\\initial.odskey',
        fileName: 'initial.odskey',
        sha256: SHA256,
      },
      initialBackup: {
        path: 'E:\\OfflineDentalSystem\\Artifacts\\Backups\\initial.odsbackup',
        fileName: 'initial.odsbackup',
        sha256: SHA256,
      },
      recoveryCode: RECOVERY_CODE,
    },
  };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('serviço HTTP do setup', () => {
  it('usa same-origin, credenciais e desabilita cache para a API', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ status: 'ok' }));
    vi.stubGlobal('fetch', fetchMock);

    await apiRequest('/health');

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/v1/health',
      expect.objectContaining({ credentials: 'same-origin', cache: 'no-store' }),
    );
  });

  it('lista somente o contrato de volumes fornecido pelo servidor', async () => {
    const volume = validVolume();
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => jsonResponse({ volumes: [volume] })),
    );

    await expect(webSetupService.listStorageVolumes()).resolves.toEqual([volume]);
  });

  it('rejeita volumes duplicados, IDs livres e números fora da faixa segura', async () => {
    const invalidVolume = {
      ...validVolume(),
      id: 'E:\\',
      availableBytes: Number.MAX_SAFE_INTEGER + 1,
    };
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => jsonResponse({ volumes: [invalidVolume, invalidVolume] })),
    );

    await expect(webSetupService.listStorageVolumes()).rejects.toMatchObject({
      code: 'INVALID_SERVER_RESPONSE',
    });
  });

  it('valida estado inicial, UUID e diagnósticos antes de liberar a interface', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ kind: 'ready', diagnostics }))
      .mockResolvedValueOnce(
        jsonResponse({
          kind: 'recovery_pending',
          setupId: 'setup-livre',
          stage: 'database',
          completedStages: [],
          diagnostics,
        }),
      );
    vi.stubGlobal('fetch', fetchMock);

    await expect(webSetupService.getStartupState()).resolves.toEqual({
      kind: 'ready',
      diagnostics,
    });
    await expect(webSetupService.getStartupState()).rejects.toMatchObject({
      code: 'INVALID_SERVER_RESPONSE',
    });
  });

  it('valida artefatos, SHA-256 e código Base64URL retornados pelo start', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(validProgress()))
      .mockResolvedValueOnce(
        jsonResponse({
          ...validProgress(),
          artifacts: {
            ...validProgress().artifacts,
            recoveryCode: 'código inválido',
            initialBackup: {
              ...validProgress().artifacts.initialBackup,
              sha256: '1234',
            },
          },
        }),
      );
    vi.stubGlobal('fetch', fetchMock);
    const input = {
      organization: { name: 'Clínica Exemplo' },
      unit: { name: 'Matriz', responsibleName: 'Responsável' },
      master: {
        fullName: 'Administrador Mestre',
        username: 'admin.master',
        email: 'admin@example.test',
        password: 'uma frase longa e segura',
        passwordConfirmation: 'uma frase longa e segura',
      },
      storage: {
        backupVolumeId: BACKUP_VOLUME_ID,
        recoveryVolumeId: RECOVERY_VOLUME_ID,
      },
    };

    await expect(webSetupService.startInitialSetup(input)).resolves.toEqual(
      validProgress(),
    );
    await expect(webSetupService.startInitialSetup(input)).rejects.toMatchObject({
      code: 'INVALID_SERVER_RESPONSE',
    });
  });

  it('envia IDs de volumes, nunca paths livres, ao retomar', async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(
        jsonResponse({
          setupId: SETUP_ID,
          stage: 'verification',
          completedStages: ['database', 'master_user'],
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          setupId: SETUP_ID,
          stage: 'verification',
          completedStages: ['database', 'database'],
        }),
      );
    vi.stubGlobal('fetch', fetchMock);

    const storage = {
      backupVolumeId: BACKUP_VOLUME_ID,
      recoveryVolumeId: RECOVERY_VOLUME_ID,
    };
    await webSetupService.resumeInitialSetup('setup/1', storage);
    expect(fetchMock).toHaveBeenLastCalledWith(
      '/api/v1/setup/setup%2F1/resume',
      expect.any(Object),
    );
    const request = fetchMock.mock.calls.at(-1)?.[1] as RequestInit;
    expect(JSON.parse(request.body as string)).toEqual({ storage });
    expect(request.body).not.toContain('Directory');

    await expect(
      webSetupService.resumeInitialSetup(SETUP_ID, storage),
    ).rejects.toMatchObject({
      code: 'INVALID_SERVER_RESPONSE',
    });
  });

  it('valida a resposta de confirmação e rejeita campos inesperados', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ kind: 'ready', diagnostics }))
      .mockResolvedValueOnce(
        jsonResponse({
          kind: 'ready',
          diagnostics,
          databaseKey: 'não deve atravessar a API',
        }),
      );
    vi.stubGlobal('fetch', fetchMock);
    const confirmation = {
      setupId: SETUP_ID,
      recoveryCode: RECOVERY_CODE,
      acknowledgedSeparateStorage: true as const,
      acknowledgedLossRisk: true as const,
    };

    await expect(webSetupService.confirmInitialSetup(confirmation)).resolves.toEqual({
      kind: 'ready',
      diagnostics,
    });
    await expect(webSetupService.confirmInitialSetup(confirmation)).rejects.toMatchObject(
      {
        code: 'INVALID_SERVER_RESPONSE',
      },
    );
  });

  it('normaliza erros por campo enviados como lista pelo backend', () => {
    expect(
      toCommandError({
        code: 'VALIDATION_FAILED',
        message: 'Dados inválidos.',
        correlationId: 'corr-1',
        fieldErrors: [{ field: 'master.username', message: 'Usuário inválido.' }],
      }),
    ).toEqual({
      code: 'VALIDATION_FAILED',
      message: 'Dados inválidos.',
      correlationId: 'corr-1',
      fieldErrors: { 'master.username': 'Usuário inválido.' },
    });
  });

  it('falha como servidor indisponível sem expor detalhes de rede', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => Promise.reject(new Error('ECONNREFUSED'))),
    );

    await expect(webSetupService.getStartupState()).rejects.toMatchObject({
      code: 'SERVER_UNAVAILABLE',
    });
  });
});
