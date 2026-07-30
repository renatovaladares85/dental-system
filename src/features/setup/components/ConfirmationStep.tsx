import { zodResolver } from '@hookform/resolvers/zod';
import { FileCheck2, FileKey2, HardDrive, KeyRound, RotateCcw } from 'lucide-react';
import { useForm } from 'react-hook-form';

import { Alert } from '../../../components/ui/Alert';
import { Button } from '../../../components/ui/Button';
import { Card } from '../../../components/ui/Card';
import { Checkbox } from '../../../components/ui/Checkbox';
import { Field } from '../../../components/ui/Field';
import { confirmationSchema, normalizeRecoveryCode } from '../schemas';
import type { ConfirmationValues } from '../schemas';
import type { CommandError, SetupArtifacts } from '../types';
import { StepHeading } from './StepHeading';

interface ConfirmationStepProps {
  artifacts: SetupArtifacts;
  busy: boolean;
  error: CommandError | null;
  onConfirm(values: ConfirmationValues): void;
  onRegenerate(): void;
}

function formatRecoveryCode(value: string): string {
  return (
    normalizeRecoveryCode(value)
      .match(/.{1,6}/g)
      ?.join(' ') ?? value
  );
}

function ArtifactCard({
  icon,
  label,
  fileName,
  path,
  checksum,
}: {
  icon: React.ReactNode;
  label: string;
  fileName: string;
  path: string;
  checksum: string;
}) {
  return (
    <div className="rounded-lg border border-emerald-200 bg-emerald-50/60 p-4">
      <div className="flex items-start gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-emerald-100 text-emerald-800">
          {icon}
        </span>
        <div className="min-w-0">
          <p className="text-xs font-semibold uppercase tracking-wide text-emerald-800">
            {label}
          </p>
          <p className="mt-1 break-all text-sm font-bold text-slate-900">{fileName}</p>
          <p className="mt-1 break-all font-mono text-[11px] leading-5 text-slate-600">
            {path}
          </p>
          <p
            className="mt-2 truncate font-mono text-[10px] text-slate-500"
            title={checksum}
          >
            SHA-256: {checksum}
          </p>
        </div>
      </div>
    </div>
  );
}

export function ConfirmationStep({
  artifacts,
  busy,
  error,
  onConfirm,
  onRegenerate,
}: ConfirmationStepProps) {
  const {
    register,
    handleSubmit,
    setError,
    formState: { errors },
  } = useForm<ConfirmationValues>({
    resolver: zodResolver(confirmationSchema),
    defaultValues: {
      recoveryCode: '',
      acknowledgedSeparateStorage: false as never,
      acknowledgedLossRisk: false as never,
    },
    mode: 'onBlur',
  });

  function validateAndConfirm(values: ConfirmationValues) {
    if (
      normalizeRecoveryCode(values.recoveryCode) !==
      normalizeRecoveryCode(artifacts.recoveryCode)
    ) {
      setError('recoveryCode', {
        type: 'validate',
        message: 'O código digitado não corresponde ao código gerado.',
      });
      return;
    }

    onConfirm({
      ...values,
      recoveryCode: normalizeRecoveryCode(values.recoveryCode),
    });
  }

  return (
    <form onSubmit={handleSubmit(validateAndConfirm)} noValidate>
      <StepHeading
        eyebrow="Etapa 5 de 5"
        title="Confirme sua recuperação"
        description="Os arquivos foram criados e verificados. Registre o código fora deste computador antes de liberar a instalação."
      />

      <Alert variant="success" title="Proteção criada com integridade verificada">
        A base, o pacote e o backup passaram pelas verificações locais. A instalação ainda
        não está liberada: conclua as confirmações abaixo.
      </Alert>

      <Card className="mt-4 p-5 sm:p-7">
        <div className="flex items-start gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-petrol-50 text-petrol-700">
            <KeyRound className="size-5" aria-hidden="true" />
          </span>
          <div>
            <h2 className="font-bold text-slate-900">Código de recuperação</h2>
            <p className="mt-1 text-sm leading-6 text-slate-500">
              Anote em local seguro. O sistema não armazena este código em texto legível.
            </p>
          </div>
        </div>
        <output
          className="mt-5 block break-all rounded-xl border border-petrol-200 bg-petrol-950 px-4 py-5 text-center font-mono text-base font-bold tracking-[0.12em] text-white sm:text-lg"
          aria-label="Código de recuperação gerado"
        >
          {formatRecoveryCode(artifacts.recoveryCode)}
        </output>
      </Card>

      <div className="mt-4 grid gap-4 sm:grid-cols-2">
        <ArtifactCard
          icon={<FileKey2 className="size-4" aria-hidden="true" />}
          label="Pacote de recuperação"
          fileName={artifacts.recoveryPackage.fileName}
          path={artifacts.recoveryPackage.path}
          checksum={artifacts.recoveryPackage.sha256}
        />
        <ArtifactCard
          icon={<HardDrive className="size-4" aria-hidden="true" />}
          label="Backup inicial"
          fileName={artifacts.initialBackup.fileName}
          path={artifacts.initialBackup.path}
          checksum={artifacts.initialBackup.sha256}
        />
      </div>

      <Card className="mt-4 grid gap-4 p-5 sm:p-7">
        <Field
          label="Digite novamente o código"
          autoComplete="off"
          spellCheck={false}
          className="font-mono"
          hint="Espaços em branco são ignorados; hífen e sublinhado fazem parte do código."
          error={errors.recoveryCode?.message}
          {...register('recoveryCode')}
        />
        <Checkbox
          label="Os dois arquivos ficarão armazenados separadamente"
          description="O pacote .odskey não deve permanecer junto do backup .odsbackup."
          error={errors.acknowledgedSeparateStorage?.message}
          {...register('acknowledgedSeparateStorage')}
        />
        <Checkbox
          label="Entendo o risco de perda definitiva"
          description="Sem a senha e os artefatos corretos, os dados cifrados não poderão ser recuperados."
          error={errors.acknowledgedLossRisk?.message}
          {...register('acknowledgedLossRisk')}
        />
      </Card>

      {error ? (
        <div className="mt-4">
          <Alert variant="danger" title="Confirmação recusada">
            <p>{error.message}</p>
            <p className="mt-1 text-xs">Referência: {error.correlationId}</p>
          </Alert>
          <Button
            className="mt-3"
            variant="secondary"
            icon={<RotateCcw className="size-4" aria-hidden="true" />}
            onClick={onRegenerate}
          >
            Voltar e regenerar artefatos
          </Button>
        </div>
      ) : null}

      <div className="mt-6 flex justify-end">
        <Button
          type="submit"
          busy={busy}
          icon={<FileCheck2 className="size-4" aria-hidden="true" />}
        >
          Confirmar e concluir
        </Button>
      </div>
    </form>
  );
}
