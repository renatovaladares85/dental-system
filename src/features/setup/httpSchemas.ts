import { z } from 'zod';

const MAX_WINDOWS_PATH_LENGTH = 32_767;
const MAX_STORAGE_VOLUMES = 64;
const UUID_LENGTH = 36;

function containsControlCharacter(value: string): boolean {
  return Array.from(value).some((character) => {
    const codePoint = character.codePointAt(0);
    return codePoint !== undefined && (codePoint <= 0x1f || codePoint === 0x7f);
  });
}

const boundedPathSchema = z
  .string()
  .min(1)
  .max(MAX_WINDOWS_PATH_LENGTH)
  .refine((value) => !containsControlCharacter(value), 'Caminho inválido.');

const artifactFileNameSchema = z
  .string()
  .min(1)
  .max(255)
  .refine(
    (value) =>
      !value.includes('\\') && !value.includes('/') && !containsControlCharacter(value),
    'Nome de arquivo inválido.',
  );

const sha256Schema = z
  .string()
  .length(64)
  .regex(/^[a-fA-F0-9]{64}$/);
const setupIdSchema = z.uuid().refine((value) => value.length === UUID_LENGTH);
const setupStageSchema = z.enum([
  'database',
  'master_user',
  'recovery_package',
  'initial_backup',
  'verification',
]);

const completedStagesSchema = z
  .array(setupStageSchema)
  .max(5)
  .refine((stages) => new Set(stages).size === stages.length, {
    message: 'Etapas concluídas duplicadas.',
  });

const semanticVersionSchema = z
  .string()
  .min(1)
  .max(64)
  .regex(/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/);
const diagnosticsShape = {
  minimumDistributionVersion: semanticVersionSchema,
  distributionReady: z.boolean(),
  keyProtection: z
    .string()
    .min(1)
    .max(64)
    .regex(/^[a-z0-9][a-z0-9._-]*$/),
};
const securityDiagnosticsSchema = z.union([
  z.object(diagnosticsShape).strict(),
  z
    .object({
      ...diagnosticsShape,
      sqlcipherVersion: semanticVersionSchema,
    })
    .strict(),
]);

const recoveryPackageSchema = z
  .object({
    path: boundedPathSchema,
    fileName: artifactFileNameSchema.regex(/\.odskey$/i),
    sha256: sha256Schema,
  })
  .strict();

const initialBackupSchema = z
  .object({
    path: boundedPathSchema,
    fileName: artifactFileNameSchema.regex(/\.odsbackup$/i),
    sha256: sha256Schema,
  })
  .strict();

const pendingArtifactsSchema = z
  .object({
    recoveryPackage: recoveryPackageSchema,
    initialBackup: initialBackupSchema,
  })
  .strict();

const setupArtifactsSchema = pendingArtifactsSchema
  .extend({
    recoveryCode: z
      .string()
      .length(48)
      .regex(/^[A-Za-z0-9_-]{48}$/),
  })
  .strict();

const setupProgressShape = {
  setupId: setupIdSchema,
  stage: setupStageSchema,
  completedStages: completedStagesSchema,
};
export const setupProgressSchema = z.union([
  z.object(setupProgressShape).strict(),
  z
    .object({
      ...setupProgressShape,
      artifacts: setupArtifactsSchema,
    })
    .strict(),
]);

const recoveryPendingShape = {
  kind: z.literal('recovery_pending'),
  setupId: setupIdSchema,
  stage: setupStageSchema,
  completedStages: completedStagesSchema,
  diagnostics: securityDiagnosticsSchema,
};
export const startupStateSchema = z.union([
  z
    .object({
      kind: z.literal('uninitialized'),
      diagnostics: securityDiagnosticsSchema,
    })
    .strict(),
  z.object(recoveryPendingShape).strict(),
  z
    .object({
      ...recoveryPendingShape,
      artifacts: pendingArtifactsSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('ready'),
      diagnostics: securityDiagnosticsSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal('recovery_required'),
      reasonCode: z.enum(['missing_key', 'invalid_key', 'database_unreadable']),
      diagnostics: securityDiagnosticsSchema,
    })
    .strict(),
]);

const storageVolumeSchema = z
  .object({
    id: z.string().regex(/^volume_[A-Za-z0-9_-]{22}$/),
    rootPath: boundedPathSchema.max(64),
    label: z.string().min(1).max(128),
    fileSystem: z
      .string()
      .min(1)
      .max(32)
      .regex(/^[A-Za-z0-9._-]+$/),
    availableBytes: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
    kind: z.enum(['fixed', 'removable']),
    writable: z.boolean(),
    destinationPath: boundedPathSchema,
  })
  .strict();

export const storageVolumesResponseSchema = z
  .object({
    volumes: z
      .array(storageVolumeSchema)
      .max(MAX_STORAGE_VOLUMES)
      .refine(
        (volumes) => new Set(volumes.map((volume) => volume.id)).size === volumes.length,
        {
          message: 'Volumes duplicados.',
        },
      ),
  })
  .strict();

export function parseSetupHttpResponse<TSchema extends z.ZodType>(
  schema: TSchema,
  value: unknown,
): z.output<TSchema> {
  const result = schema.safeParse(value);
  if (result.success) return result.data;

  throw {
    code: 'INVALID_SERVER_RESPONSE',
    message: 'O servidor retornou uma resposta de configuração inválida.',
    correlationId: crypto.randomUUID(),
  };
}
