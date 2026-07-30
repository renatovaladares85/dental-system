import { useCallback, useEffect, useState } from 'react';

import { ClinicStep } from './components/ClinicStep';
import { ConfirmationStep } from './components/ConfirmationStep';
import { MasterStep } from './components/MasterStep';
import { SecurityStep } from './components/SecurityStep';
import type { StorageSelection } from './components/SecurityStep';
import { SetupLayout } from './components/SetupLayout';
import { WelcomeStep } from './components/WelcomeStep';
import type {
  ClinicFormValues,
  ClinicValues,
  ConfirmationValues,
  MasterValues,
} from './schemas';
import { toCommandError } from './service';
import type {
  CommandError,
  InitialSetupInput,
  SetupProgress,
  SetupService,
  StartupState,
  StorageVolume,
} from './types';

interface SetupWizardProps {
  service: SetupService;
  initialState: Extract<StartupState, { kind: 'uninitialized' | 'recovery_pending' }>;
  onReady(state: Extract<StartupState, { kind: 'ready' }>): void;
}

const initialClinicValues: ClinicFormValues = {
  organizationName: '',
  unitName: 'Unidade principal',
  responsibleName: '',
  phone: '',
  administrativeEmail: '',
  address: '',
  professionalRegistration: '',
};

const initialMasterValues: MasterValues = {
  fullName: '',
  username: '',
  email: '',
  password: '',
  passwordConfirmation: '',
};

const initialStorage: StorageSelection = {
  backupVolumeId: '',
  recoveryVolumeId: '',
};

function localError(
  code: string,
  message: string,
  fieldErrors?: Record<string, string>,
): CommandError {
  return {
    code,
    message,
    correlationId: `local-${Date.now().toString(36)}`,
    ...(fieldErrors ? { fieldErrors } : {}),
  };
}

function errorTargetStep(error: CommandError): 2 | 3 | 4 | null {
  const fields = Object.keys(error.fieldErrors ?? {});
  if (
    fields.some((field) => field.startsWith('organization.') || field.startsWith('unit.'))
  ) {
    return 2;
  }
  if (fields.some((field) => field.startsWith('master.'))) return 3;
  if (fields.some((field) => field === 'storage' || field.startsWith('storage.')))
    return 4;
  return null;
}

function areDifferentDirectories(storage: StorageSelection): boolean {
  return storage.backupVolumeId !== storage.recoveryVolumeId;
}

function optionalProperty<K extends string>(
  key: K,
  value: string,
): Partial<Record<K, string>> {
  const normalized = value.trim();
  return normalized.length > 0 ? ({ [key]: normalized } as Record<K, string>) : {};
}

export function SetupWizard({ service, initialState, onReady }: SetupWizardProps) {
  const initialPending = initialState.kind === 'recovery_pending' ? initialState : null;
  const [step, setStep] = useState(initialPending ? 4 : 1);
  const [clinic, setClinic] = useState<ClinicValues>(initialClinicValues);
  const [master, setMaster] = useState<MasterValues>(initialMasterValues);
  const [storage, setStorage] = useState<StorageSelection>(initialStorage);
  const [volumes, setVolumes] = useState<StorageVolume[]>([]);
  const [volumesStatus, setVolumesStatus] = useState<'idle' | 'loading' | 'loaded'>(
    'idle',
  );
  const [pendingSetupId, setPendingSetupId] = useState<string | null>(
    initialPending?.setupId ?? null,
  );
  const [progress, setProgress] = useState<SetupProgress | null>(() =>
    initialPending
      ? {
          setupId: initialPending.setupId,
          stage: initialPending.stage,
          completedStages: initialPending.completedStages,
        }
      : null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const volumesLoading = volumesStatus === 'loading';
  const loadVolumes = useCallback(async () => {
    setVolumesStatus('loading');
    setError(null);
    try {
      setVolumes(await service.listStorageVolumes());
    } catch (caught) {
      setError(toCommandError(caught));
    } finally {
      setVolumesStatus('loaded');
    }
  }, [service]);

  useEffect(() => {
    if (step !== 4 || volumesStatus !== 'idle') return;
    let active = true;
    queueMicrotask(() => {
      if (active) void loadVolumes();
    });
    return () => {
      active = false;
    };
  }, [loadVolumes, step, volumesStatus]);

  function buildInput(): InitialSetupInput {
    return {
      organization: { name: clinic.organizationName.trim() },
      unit: {
        name: clinic.unitName.trim(),
        responsibleName: clinic.responsibleName.trim(),
        ...optionalProperty('phone', clinic.phone),
        ...optionalProperty('administrativeEmail', clinic.administrativeEmail),
        ...optionalProperty('address', clinic.address),
        ...optionalProperty('professionalRegistration', clinic.professionalRegistration),
      },
      master: {
        fullName: master.fullName.trim(),
        username: master.username.trim(),
        email: master.email.trim().toLocaleLowerCase('pt-BR'),
        password: master.password,
        passwordConfirmation: master.passwordConfirmation,
      },
      storage,
    };
  }

  function acceptProgress(result: SetupProgress) {
    setProgress(result);
    setPendingSetupId(result.setupId);
    setMaster((current) => ({ ...current, password: '', passwordConfirmation: '' }));

    if (result.artifacts) {
      setStep(5);
    }
  }

  async function generateSetup() {
    setError(null);
    if (!areDifferentDirectories(storage)) {
      setError(
        localError(
          'DIRECTORIES_MUST_DIFFER',
          'Escolha volumes diferentes para o backup e o pacote de recuperação.',
        ),
      );
      return;
    }

    setBusy(true);
    try {
      acceptProgress(await service.startInitialSetup(buildInput()));
    } catch (caught) {
      const commandError = toCommandError(caught);
      setError(commandError);
      let pendingDetected = false;

      try {
        const state = await service.getStartupState();
        if (state.kind === 'recovery_pending') {
          pendingDetected = true;
          setPendingSetupId(state.setupId);
          setProgress({
            setupId: state.setupId,
            stage: state.stage,
            completedStages: state.completedStages,
          });
          setMaster((current) => ({
            ...current,
            password: '',
            passwordConfirmation: '',
          }));
          setStep(4);
        }
      } catch {
        // The original sanitized command error remains the actionable feedback.
      }

      if (!pendingDetected) {
        const targetStep = errorTargetStep(commandError);
        if (targetStep) setStep(targetStep);
      }
    } finally {
      setBusy(false);
    }
  }

  async function resumeSetup() {
    if (!pendingSetupId) {
      setError(
        localError(
          'MISSING_SETUP_ID',
          'Não foi possível identificar a configuração pendente.',
        ),
      );
      return;
    }

    const hasBackupVolume = storage.backupVolumeId.length > 0;
    const hasRecoveryVolume = storage.recoveryVolumeId.length > 0;
    if (!hasBackupVolume || !hasRecoveryVolume) {
      setError(
        localError(
          'STORAGE_SELECTION_REQUIRED',
          'Selecione novamente os dois volumes para retomar com segurança.',
          {
            ...(!hasBackupVolume
              ? { 'storage.backupVolumeId': 'Selecione também o volume do backup.' }
              : {}),
            ...(!hasRecoveryVolume
              ? {
                  'storage.recoveryVolumeId':
                    'Selecione também o volume do pacote de recuperação.',
                }
              : {}),
          },
        ),
      );
      return;
    }
    if (!areDifferentDirectories(storage)) {
      setError(
        localError(
          'DIRECTORIES_MUST_DIFFER',
          'Escolha volumes diferentes para o backup e o pacote de recuperação.',
          { storage: 'Os destinos precisam ser diferentes.' },
        ),
      );
      return;
    }

    setError(null);
    setBusy(true);
    try {
      const result = await service.resumeInitialSetup(pendingSetupId, storage);
      acceptProgress(result);

      if (!result.artifacts) {
        setError(
          localError(
            'ARTIFACTS_NOT_AVAILABLE',
            'A retomada ainda não disponibilizou os artefatos. Execute a retomada novamente.',
          ),
        );
      }
    } catch (caught) {
      setError(toCommandError(caught));
    } finally {
      setBusy(false);
    }
  }

  function returnToRegenerateArtifacts() {
    setProgress((current) =>
      current
        ? {
            setupId: current.setupId,
            stage: current.stage,
            completedStages: current.completedStages,
          }
        : null,
    );
    setError(null);
    setStep(4);
  }

  async function confirmSetup(values: ConfirmationValues) {
    if (!progress?.artifacts || !pendingSetupId) return;

    setError(null);
    setBusy(true);
    try {
      const state = await service.confirmInitialSetup({
        setupId: pendingSetupId,
        recoveryCode: values.recoveryCode,
        acknowledgedSeparateStorage: true,
        acknowledgedLossRisk: true,
      });

      if (state.kind !== 'ready') {
        setError(
          localError(
            'SETUP_NOT_READY',
            'A verificação terminou, mas a instalação ainda não foi liberada.',
          ),
        );
        return;
      }

      setMaster(initialMasterValues);
      setProgress(null);
      onReady(state);
    } catch (caught) {
      setError(toCommandError(caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <SetupLayout currentStep={step} diagnostics={initialState.diagnostics}>
      {step === 1 ? <WelcomeStep onStart={() => setStep(2)} /> : null}
      {step === 2 ? (
        <ClinicStep
          initialValues={clinic}
          commandError={error}
          onBack={() => setStep(1)}
          onContinue={(values) => {
            setClinic(values);
            setStep(3);
          }}
        />
      ) : null}
      {step === 3 ? (
        <MasterStep
          initialValues={master}
          commandError={error}
          onBack={() => setStep(2)}
          onContinue={(values) => {
            setMaster(values);
            setStep(4);
          }}
        />
      ) : null}
      {step === 4 ? (
        <SecurityStep
          storage={storage}
          progress={progress}
          pendingSetup={pendingSetupId !== null}
          busy={busy}
          volumes={volumes}
          volumesLoading={volumesLoading}
          error={error}
          onBack={() => setStep(3)}
          onSelectBackup={(backupVolumeId) =>
            setStorage((current) => ({ ...current, backupVolumeId }))
          }
          onSelectRecovery={(recoveryVolumeId) =>
            setStorage((current) => ({ ...current, recoveryVolumeId }))
          }
          onReloadVolumes={() => void loadVolumes()}
          onGenerate={() => void generateSetup()}
          onResume={() => void resumeSetup()}
        />
      ) : null}
      {step === 5 && progress?.artifacts ? (
        <ConfirmationStep
          artifacts={progress.artifacts}
          busy={busy}
          error={error}
          onConfirm={(values) => void confirmSetup(values)}
          onRegenerate={returnToRegenerateArtifacts}
        />
      ) : null}
    </SetupLayout>
  );
}
