export interface PairingMaterial {
  token: string;
  expectedFingerprintSha256: string;
}

export interface PairingCompletion {
  fingerprintSha256: string;
  caCertificateDerBase64: string;
  caFileName: string;
  serverUrl: string;
}

export interface PairingService {
  complete(token: string): Promise<PairingCompletion>;
}
