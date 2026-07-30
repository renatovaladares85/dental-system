export type SetupStage =
  'database' | 'master_user' | 'recovery_package' | 'initial_backup' | 'verification';

export interface SecurityDiagnostics {
  sqlcipherVersion?: string;
  minimumDistributionVersion: string;
  distributionReady: boolean;
  keyProtection: string;
}

export interface ArtifactDescriptor {
  path: string;
  fileName: string;
  sha256: string;
}

export interface SetupArtifacts {
  recoveryPackage: ArtifactDescriptor;
  initialBackup: ArtifactDescriptor;
  recoveryCode: string;
}

export interface RecoveryPendingArtifacts {
  recoveryPackage: ArtifactDescriptor;
  initialBackup: ArtifactDescriptor;
}

export type RecoveryReasonCode = 'missing_key' | 'invalid_key' | 'database_unreadable';

export type StartupState =
  | { kind: 'uninitialized'; diagnostics: SecurityDiagnostics }
  | {
      kind: 'recovery_pending';
      setupId: string;
      stage: SetupStage;
      completedStages: SetupStage[];
      artifacts?: RecoveryPendingArtifacts;
      diagnostics: SecurityDiagnostics;
    }
  | { kind: 'ready'; diagnostics: SecurityDiagnostics }
  | {
      kind: 'recovery_required';
      reasonCode: RecoveryReasonCode;
      diagnostics: SecurityDiagnostics;
    };

export interface OrganizationInput {
  name: string;
}

export interface UnitInput {
  name: string;
  responsibleName: string;
  phone?: string;
  administrativeEmail?: string;
  address?: string;
  professionalRegistration?: string;
}

export interface MasterInput {
  fullName: string;
  username: string;
  email: string;
  password: string;
  passwordConfirmation: string;
}

export interface StorageInput {
  backupVolumeId: string;
  recoveryVolumeId: string;
}

export interface StorageVolume {
  id: string;
  rootPath: string;
  label: string;
  fileSystem: string;
  availableBytes: number;
  kind: 'fixed' | 'removable';
  writable: boolean;
  destinationPath: string;
}

export interface InitialSetupInput {
  organization: OrganizationInput;
  unit: UnitInput;
  master: MasterInput;
  storage: StorageInput;
}

export interface SetupProgress {
  setupId: string;
  stage: SetupStage;
  completedStages: SetupStage[];
  artifacts?: SetupArtifacts;
}

export interface ConfirmSetupInput {
  setupId: string;
  recoveryCode: string;
  acknowledgedSeparateStorage: true;
  acknowledgedLossRisk: true;
}

export interface PairingSession {
  token: string;
  pairingUrl: string;
  fingerprintSha256: string;
  expiresAt: string;
}

export interface CommandError {
  code: string;
  message: string;
  correlationId: string;
  fieldErrors?: Record<string, string>;
}

export interface SetupService {
  isSetupOrigin(): boolean;
  getStartupState(): Promise<StartupState>;
  listStorageVolumes(): Promise<StorageVolume[]>;
  startInitialSetup(input: InitialSetupInput): Promise<SetupProgress>;
  resumeInitialSetup(setupId: string, storage: StorageInput): Promise<SetupProgress>;
  confirmInitialSetup(input: ConfirmSetupInput): Promise<StartupState>;
  startPairing(): Promise<PairingSession>;
}
