import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { SetupWizard } from './SetupWizard';
import type {
  ConfirmSetupInput,
  InitialSetupInput,
  SetupProgress,
  SetupService,
  StartupState,
  StorageInput,
} from './types';

const diagnostics = {
  sqlcipherVersion: '4.17.0',
  minimumDistributionVersion: '4.17.0',
  distributionReady: true,
  keyProtection: 'dpapi-current-user',
};

const recoveryCode = 'AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcdefghij';
const regeneratedCode = 'ZyXwVuTsRqPoNmLkJiHgFeDcBa9876543210-_abcdefghij';
const volumes = [
  {
    id: 'volume:backup',
    rootPath: 'E:\\',
    label: 'Backup',
    fileSystem: 'NTFS',
    availableBytes: 80_000_000_000,
    kind: 'removable' as const,
    writable: true,
    destinationPath: 'E:\\OfflineDentalSystem\\Artifacts',
  },
  {
    id: 'volume:recovery',
    rootPath: 'D:\\',
    label: 'Recuperação',
    fileSystem: 'NTFS',
    availableBytes: 40_000_000_000,
    kind: 'fixed' as const,
    writable: true,
    destinationPath: 'D:\\OfflineDentalSystem\\Artifacts',
  },
];

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolver) => {
    resolve = resolver;
  });

  return { promise, resolve };
}

function progressWithArtifacts(code = recoveryCode, setupId = 'setup-1'): SetupProgress {
  return {
    setupId,
    stage: 'verification',
    completedStages: ['database', 'master_user', 'recovery_package', 'initial_backup'],
    artifacts: {
      recoveryCode: code,
      recoveryPackage: {
        path: 'D:\\Recovery\\clinic.odskey',
        fileName: 'clinic.odskey',
        sha256: 'a'.repeat(64),
      },
      initialBackup: {
        path: 'E:\\Backup\\clinic.odsbackup',
        fileName: 'clinic.odsbackup',
        sha256: 'b'.repeat(64),
      },
    },
  };
}

function createService(overrides: Partial<SetupService> = {}) {
  const startInitialSetup = vi.fn(
    async (input: InitialSetupInput): Promise<SetupProgress> => {
      void input;
      return progressWithArtifacts();
    },
  );
  const confirmInitialSetup = vi.fn(async (input: ConfirmSetupInput) => {
    void input;
    return {
      kind: 'ready' as const,
      diagnostics,
    };
  });

  const service: SetupService = {
    isSetupOrigin: () => true,
    getStartupState: vi.fn(async () => ({ kind: 'uninitialized' as const, diagnostics })),
    listStorageVolumes: vi.fn(async () => volumes),
    startInitialSetup,
    resumeInitialSetup: vi.fn(async () => {
      throw new Error('não esperado');
    }),
    confirmInitialSetup,
    startPairing: vi.fn(),
    ...overrides,
  };

  return { service, startInitialSetup, confirmInitialSetup };
}

async function enterSetupDetails(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'Configurar novo sistema' }));
  await user.type(screen.getByLabelText('Nome da clínica'), 'Clínica Horizonte');
  await user.type(screen.getByLabelText('Responsável pela unidade'), 'Marina Costa');
  await user.click(screen.getByRole('button', { name: 'Continuar' }));

  await user.type(screen.getByLabelText('Nome completo'), 'Marina Costa');
  await user.type(screen.getByLabelText('Nome de usuário'), 'marina.admin');
  await user.type(screen.getByLabelText('E-mail'), 'marina@example.test');
  await user.type(screen.getByLabelText('Senha'), 'Horizonte sereno depois da chuva');
  await user.type(
    screen.getByLabelText('Confirmar senha'),
    'Horizonte sereno depois da chuva',
  );
  await user.click(screen.getByRole('button', { name: 'Continuar' }));
}

async function chooseInitialDirectories(user: ReturnType<typeof userEvent.setup>) {
  const selectors = await screen.findAllByRole('combobox');
  await user.selectOptions(selectors[0]!, 'volume:backup');
  await user.selectOptions(selectors[1]!, 'volume:recovery');
}

async function confirmDisplayedCode(
  user: ReturnType<typeof userEvent.setup>,
  code: string,
) {
  await user.type(screen.getByLabelText('Digite novamente o código'), code);
  await user.click(
    screen.getByRole('checkbox', {
      name: /Os dois arquivos ficarão armazenados separadamente/,
    }),
  );
  await user.click(
    screen.getByRole('checkbox', { name: /Entendo o risco de perda definitiva/ }),
  );
  await user.click(screen.getByRole('button', { name: 'Confirmar e concluir' }));
}

describe('configuração inicial', () => {
  it('permite iniciar pelo teclado e move o foco para o primeiro campo', async () => {
    const user = userEvent.setup();
    const { service } = createService();
    const initialState: Extract<StartupState, { kind: 'uninitialized' }> = {
      kind: 'uninitialized',
      diagnostics,
    };

    render(
      <SetupWizard service={service} initialState={initialState} onReady={vi.fn()} />,
    );

    await user.tab();
    expect(screen.getByRole('link', { name: 'Ir para o conteúdo' })).toHaveFocus();
    await user.tab();
    expect(
      screen.getByRole('button', { name: 'Tenho um pacote de recuperação' }),
    ).toHaveFocus();
    await user.tab();
    expect(screen.getByRole('button', { name: 'Configurar novo sistema' })).toHaveFocus();

    await user.keyboard('{Enter}');

    expect(await screen.findByLabelText('Nome da clínica')).toHaveFocus();
  });

  it('bloqueia ações e anuncia o processamento enquanto o setup está em execução', async () => {
    const user = userEvent.setup();
    const pendingSetup = deferred<SetupProgress>();
    const startInitialSetup = vi.fn(() => pendingSetup.promise);
    const { service } = createService({ startInitialSetup });
    const initialState: Extract<StartupState, { kind: 'uninitialized' }> = {
      kind: 'uninitialized',
      diagnostics,
    };

    render(
      <SetupWizard service={service} initialState={initialState} onReady={vi.fn()} />,
    );
    await enterSetupDetails(user);
    await chooseInitialDirectories(user);

    const generate = screen.getByRole('button', {
      name: 'Gerar proteção e backup',
    });
    await user.click(generate);

    expect(generate).toBeDisabled();
    expect(generate).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByText('Processando com segurança…')).toBeInTheDocument();

    pendingSetup.resolve(progressWithArtifacts());
    expect(await screen.findByText('Confirme sua recuperação')).toBeInTheDocument();
  });

  it('conclui as cinco etapas usando somente os comandos tipados', async () => {
    const user = userEvent.setup();
    const { service, startInitialSetup, confirmInitialSetup } = createService();
    const onReady = vi.fn();
    const initialState: Extract<StartupState, { kind: 'uninitialized' }> = {
      kind: 'uninitialized',
      diagnostics,
    };

    render(
      <SetupWizard service={service} initialState={initialState} onReady={onReady} />,
    );

    await enterSetupDetails(user);
    await chooseInitialDirectories(user);
    await user.click(screen.getByRole('button', { name: 'Gerar proteção e backup' }));

    expect(await screen.findByText('Confirme sua recuperação')).toBeInTheDocument();
    expect(startInitialSetup).toHaveBeenCalledWith(
      expect.objectContaining({
        organization: { name: 'Clínica Horizonte' },
        storage: {
          backupVolumeId: 'volume:backup',
          recoveryVolumeId: 'volume:recovery',
        },
      }),
    );

    await confirmDisplayedCode(user, recoveryCode);

    await waitFor(() => expect(confirmInitialSetup).toHaveBeenCalledTimes(1));
    expect(onReady).toHaveBeenCalledWith({ kind: 'ready', diagnostics });
  });

  it('reexige os volumes e retoma estado pendente somente com IDs atuais', async () => {
    const user = userEvent.setup();
    const resumeInitialSetup = vi.fn(
      async (setupId: string, storage?: StorageInput): Promise<SetupProgress> => {
        void storage;
        return progressWithArtifacts(regeneratedCode, setupId);
      },
    );
    const { service } = createService({ resumeInitialSetup });
    const initialState: Extract<StartupState, { kind: 'recovery_pending' }> = {
      kind: 'recovery_pending',
      setupId: 'setup-pending',
      stage: 'verification',
      completedStages: ['database', 'master_user', 'recovery_package', 'initial_backup'],
      diagnostics,
    };

    render(
      <SetupWizard service={service} initialState={initialState} onReady={vi.fn()} />,
    );

    const selectors = await screen.findAllByRole('combobox');
    expect(screen.getByRole('button', { name: 'Retomar configuração' })).toBeDisabled();
    await user.selectOptions(selectors[0]!, 'volume:backup');
    await user.selectOptions(selectors[1]!, 'volume:recovery');
    await user.click(screen.getByRole('button', { name: 'Retomar configuração' }));

    await waitFor(() =>
      expect(resumeInitialSetup).toHaveBeenCalledWith('setup-pending', {
        backupVolumeId: 'volume:backup',
        recoveryVolumeId: 'volume:recovery',
      }),
    );
    expect(await screen.findByText('Confirme sua recuperação')).toBeInTheDocument();
  });

  it('exige os dois diretórios quando o estado pendente recebe override parcial', async () => {
    const user = userEvent.setup();
    const { service } = createService();
    const initialState: Extract<StartupState, { kind: 'recovery_pending' }> = {
      kind: 'recovery_pending',
      setupId: 'setup-pending',
      stage: 'recovery_package',
      completedStages: ['database', 'master_user'],
      diagnostics,
    };

    render(
      <SetupWizard service={service} initialState={initialState} onReady={vi.fn()} />,
    );
    const selectors = await screen.findAllByRole('combobox');
    await user.selectOptions(selectors[0]!, 'volume:backup');

    expect(screen.getByText('Selecione os dois destinos')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retomar configuração' })).toBeDisabled();
  });

  it('roteia e exibe erros canônicos do backend no formulário mestre', async () => {
    const user = userEvent.setup();
    const startInitialSetup = vi.fn(async () => {
      throw {
        code: 'VALIDATION_FAILED',
        message: 'Revise os campos destacados e tente novamente.',
        correlationId: 'corr-master-1',
        fieldErrors: {
          'master.password': 'A senha não pode conter o nome da clínica.',
        },
      };
    });
    const { service } = createService({ startInitialSetup });
    const initialState: Extract<StartupState, { kind: 'uninitialized' }> = {
      kind: 'uninitialized',
      diagnostics,
    };

    render(
      <SetupWizard service={service} initialState={initialState} onReady={vi.fn()} />,
    );
    await enterSetupDetails(user);
    await chooseInitialDirectories(user);
    await user.click(screen.getByRole('button', { name: 'Gerar proteção e backup' }));

    expect(await screen.findByText('Crie o administrador mestre')).toBeInTheDocument();
    expect(
      screen.getByText('A senha não pode conter o nome da clínica.'),
    ).toBeInTheDocument();
    expect(screen.getByText('Referência: corr-master-1')).toBeInTheDocument();
  });

  it('permite voltar da falha de confirmação e regenerar os artefatos', async () => {
    const user = userEvent.setup();
    const confirmInitialSetup = vi.fn(async () => {
      throw {
        code: 'SECURITY_OPERATION_FAILED',
        message: 'Os artefatos não passaram pela verificação.',
        correlationId: 'corr-artifact-1',
      };
    });
    const resumeInitialSetup = vi.fn(
      async (setupId: string, storage?: StorageInput): Promise<SetupProgress> => {
        void storage;
        return progressWithArtifacts(regeneratedCode, setupId);
      },
    );
    const { service } = createService({ confirmInitialSetup, resumeInitialSetup });
    const initialState: Extract<StartupState, { kind: 'uninitialized' }> = {
      kind: 'uninitialized',
      diagnostics,
    };

    render(
      <SetupWizard service={service} initialState={initialState} onReady={vi.fn()} />,
    );
    await enterSetupDetails(user);
    await chooseInitialDirectories(user);
    await user.click(screen.getByRole('button', { name: 'Gerar proteção e backup' }));
    await screen.findByText('Confirme sua recuperação');
    await confirmDisplayedCode(user, recoveryCode);

    const regenerate = await screen.findByRole('button', {
      name: 'Voltar e regenerar artefatos',
    });
    await user.click(regenerate);
    expect(screen.getByText('Proteja a instalação')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retomar configuração' }));

    expect(await screen.findByText('Confirme sua recuperação')).toBeInTheDocument();
    expect(resumeInitialSetup).toHaveBeenCalledWith('setup-1', {
      backupVolumeId: 'volume:backup',
      recoveryVolumeId: 'volume:recovery',
    });
  });
});
