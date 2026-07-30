import {
  ArrowLeft,
  Check,
  CircleDashed,
  Database,
  FileKey2,
  HardDrive,
  KeyRound,
  RotateCcw,
  ShieldCheck,
} from 'lucide-react';
import type { ReactNode } from 'react';

import { Alert } from '../../../components/ui/Alert';
import { Button } from '../../../components/ui/Button';
import { Card } from '../../../components/ui/Card';
import { cn } from '../../../components/ui/cn';
import type { CommandError, SetupProgress, SetupStage, StorageVolume } from '../types';
import { StepHeading } from './StepHeading';

export interface StorageSelection {
  backupVolumeId: string;
  recoveryVolumeId: string;
}

interface SecurityStepProps {
  storage: StorageSelection;
  progress: SetupProgress | null;
  pendingSetup: boolean;
  busy: boolean;
  volumes: StorageVolume[];
  volumesLoading: boolean;
  error: CommandError | null;
  onBack(): void;
  onSelectBackup(volumeId: string): void;
  onSelectRecovery(volumeId: string): void;
  onReloadVolumes(): void;
  onGenerate(): void;
  onResume(): void;
}

const stages: Array<{ id: SetupStage; label: string; icon: ReactNode }> = [
  { id: 'database', label: 'Banco protegido', icon: <Database className="size-4" /> },
  {
    id: 'master_user',
    label: 'Administrador mestre',
    icon: <ShieldCheck className="size-4" />,
  },
  {
    id: 'recovery_package',
    label: 'Pacote de recuperação',
    icon: <FileKey2 className="size-4" />,
  },
  {
    id: 'initial_backup',
    label: 'Backup inicial',
    icon: <HardDrive className="size-4" />,
  },
  {
    id: 'verification',
    label: 'Verificação de integridade',
    icon: <Check className="size-4" />,
  },
];

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return 'espaço indisponível';
  return new Intl.NumberFormat('pt-BR', {
    style: 'unit',
    unit: 'gigabyte',
    maximumFractionDigits: 1,
  }).format(bytes / 1024 ** 3);
}

function VolumeCard({
  label,
  description,
  selectedId,
  emptyValue,
  error,
  disabled,
  volumes,
  onSelect,
}: {
  label: string;
  description: string;
  selectedId: string;
  emptyValue: string;
  error?: string | undefined;
  disabled: boolean;
  volumes: StorageVolume[];
  onSelect(volumeId: string): void;
}) {
  const selected = volumes.find((volume) => volume.id === selectedId);

  return (
    <div className="rounded-lg border border-slate-200 p-4">
      <label className="block text-sm font-bold text-slate-900">
        {label}
        <span className="mt-1 block text-xs font-normal leading-5 text-slate-500">
          {description}
        </span>
        <select
          className="mt-3 min-h-11 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-950 outline-none focus:border-petrol-500 focus:ring-3 focus:ring-petrol-100 disabled:bg-slate-100"
          value={selectedId}
          disabled={disabled}
          onChange={(event) => onSelect(event.target.value)}
        >
          <option value="">{emptyValue}</option>
          {volumes.map((volume) => (
            <option key={volume.id} value={volume.id} disabled={!volume.writable}>
              {volume.label || volume.rootPath} · {formatBytes(volume.availableBytes)}
              {!volume.writable ? ' · somente leitura' : ''}
            </option>
          ))}
        </select>
      </label>
      <div
        className={`mt-3 min-h-10 rounded-lg px-3 py-2 font-mono text-xs leading-5 ${
          error ? 'bg-red-50 text-red-800' : 'bg-slate-100 text-slate-600'
        }`}
      >
        {selected
          ? `${selected.destinationPath} · ${selected.fileSystem} · ${
              selected.kind === 'removable' ? 'removível' : 'local'
            }`
          : emptyValue}
      </div>
      {error ? <p className="mt-1.5 text-sm text-red-700">{error}</p> : null}
    </div>
  );
}

export function SecurityStep({
  storage,
  progress,
  pendingSetup,
  busy,
  volumes,
  volumesLoading,
  error,
  onBack,
  onSelectBackup,
  onSelectRecovery,
  onReloadVolumes,
  onGenerate,
  onResume,
}: SecurityStepProps) {
  const directoriesReady =
    storage.backupVolumeId.length > 0 && storage.recoveryVolumeId.length > 0;
  const hasBackupOverride = storage.backupVolumeId.length > 0;
  const hasRecoveryOverride = storage.recoveryVolumeId.length > 0;
  const partialOverride = pendingSetup && hasBackupOverride !== hasRecoveryOverride;

  return (
    <div>
      <StepHeading
        eyebrow="Etapa 4 de 5"
        title="Proteja a instalação"
        description="O sistema criará uma chave exclusiva, o banco cifrado, um pacote de recuperação e o primeiro backup."
      />

      {pendingSetup ? (
        <Alert variant="warning" title="Configuração aguardando confirmação">
          Uma preparação anterior foi interrompida. Retome o processo para regenerar um
          código de recuperação. Por segurança, confirme novamente dois volumes locais
          diferentes; caminhos persistidos nunca são reutilizados pela API web.
        </Alert>
      ) : null}

      <Card className={`${pendingSetup ? 'mt-4' : ''} p-5 sm:p-7`}>
        <div className="mb-5 flex items-start gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-petrol-50 text-petrol-700">
            <KeyRound className="size-5" aria-hidden="true" />
          </span>
          <div>
            <h2 className="font-bold text-slate-900">Destinos dos artefatos</h2>
            <p className="mt-1 text-sm leading-6 text-slate-500">
              Selecione volumes locais diferentes. Os diretórios são definidos e validados
              pelo servidor; caminhos livres e unidades de rede não são aceitos.
            </p>
          </div>
        </div>

        <div className="grid gap-4">
          <VolumeCard
            label="Backup inicial"
            description="Snapshot cifrado da base no formato .odsbackup."
            selectedId={storage.backupVolumeId}
            emptyValue="Selecione um volume"
            error={error?.fieldErrors?.['storage.backupVolumeId']}
            disabled={busy || volumesLoading}
            volumes={volumes}
            onSelect={onSelectBackup}
          />
          <VolumeCard
            label="Pacote de recuperação"
            description="Envelope da chave no formato .odskey, protegido pelo código."
            selectedId={storage.recoveryVolumeId}
            emptyValue="Selecione um volume"
            error={error?.fieldErrors?.['storage.recoveryVolumeId']}
            disabled={busy || volumesLoading}
            volumes={volumes}
            onSelect={onSelectRecovery}
          />
        </div>

        {volumes.length === 0 && !volumesLoading ? (
          <Alert className="mt-4" variant="warning" title="Nenhum volume disponível">
            Disponibilize dois volumes locais graváveis, por exemplo o disco local e uma
            mídia externa, e tente a detecção novamente.
            <Button
              className="mt-3"
              variant="secondary"
              icon={<RotateCcw className="size-4" aria-hidden="true" />}
              onClick={onReloadVolumes}
            >
              Detectar novamente
            </Button>
          </Alert>
        ) : null}
      </Card>

      {partialOverride ? (
        <Alert className="mt-4" variant="danger" title="Selecione os dois destinos">
          Para substituir os destinos persistidos, escolha um volume para o backup e outro
          para o pacote de recuperação.
        </Alert>
      ) : null}

      {progress || busy ? (
        <Card className="mt-4 p-5" aria-live="polite">
          <h2 className="text-sm font-bold text-slate-900">
            {busy ? 'Processando com segurança…' : 'Progresso da preparação'}
          </h2>
          <ol className="mt-4 grid gap-2 sm:grid-cols-5">
            {stages.map((stage) => {
              const complete = progress?.completedStages.includes(stage.id) ?? false;
              const active = busy && (progress?.stage ?? 'database') === stage.id;

              return (
                <li
                  key={stage.id}
                  className={cn(
                    'flex items-center gap-2 rounded-lg border p-3 text-xs font-semibold sm:flex-col sm:text-center',
                    complete && 'border-emerald-200 bg-emerald-50 text-emerald-800',
                    active && 'border-petrol-300 bg-petrol-50 text-petrol-800',
                    !complete && !active && 'border-slate-200 text-slate-500',
                  )}
                >
                  <span aria-hidden="true">
                    {complete ? (
                      <Check className="size-4" />
                    ) : active ? (
                      <CircleDashed className="size-4 animate-spin" />
                    ) : (
                      stage.icon
                    )}
                  </span>
                  {stage.label}
                </li>
              );
            })}
          </ol>
        </Card>
      ) : null}

      {error ? (
        <Alert className="mt-4" variant="danger" title="Não foi possível concluir">
          <p>{error.message}</p>
          <p className="mt-1 text-xs">Referência: {error.correlationId}</p>
        </Alert>
      ) : null}

      <div className="mt-6 flex items-center justify-between gap-3">
        <Button
          variant="ghost"
          icon={<ArrowLeft className="size-4" aria-hidden="true" />}
          disabled={busy || pendingSetup}
          onClick={onBack}
        >
          Voltar
        </Button>
        {pendingSetup ? (
          <Button
            icon={<RotateCcw className="size-4" aria-hidden="true" />}
            busy={busy}
            disabled={!directoriesReady || volumesLoading}
            onClick={onResume}
          >
            Retomar configuração
          </Button>
        ) : (
          <Button
            icon={<ShieldCheck className="size-4" aria-hidden="true" />}
            busy={busy}
            disabled={!directoriesReady || volumesLoading}
            onClick={onGenerate}
          >
            Gerar proteção e backup
          </Button>
        )}
      </div>
    </div>
  );
}
