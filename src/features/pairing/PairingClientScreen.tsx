import { Download, ExternalLink, Fingerprint, ShieldCheck } from 'lucide-react';
import { useEffect, useState } from 'react';

import { AppLogo } from '../../components/brand/AppLogo';
import { Alert } from '../../components/ui/Alert';
import { Card } from '../../components/ui/Card';
import { toApiError } from '../../lib/api';
import type { ApiError } from '../../lib/api';
import type { PairingMaterial, PairingService } from './types';

interface VerifiedPairing {
  certificateUrl: string;
  certificateFileName: string;
  fingerprintSha256: string;
  serverUrl: string;
}

type VerificationState =
  | { status: 'verifying' }
  | { status: 'verified'; result: VerifiedPairing }
  | { status: 'error'; error: ApiError };

function pairingError(code: string, message: string): ApiError {
  return { code, message, correlationId: crypto.randomUUID() };
}

function decodeCertificate(value: string): Uint8Array<ArrayBuffer> {
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) {
    throw pairingError('INVALID_CA_CERTIFICATE', 'O certificado recebido é inválido.');
  }

  const binary = atob(value);
  if (binary.length === 0 || binary.length > 65_536) {
    throw pairingError('INVALID_CA_CERTIFICATE', 'O certificado recebido é inválido.');
  }

  const certificate = new Uint8Array(new ArrayBuffer(binary.length));
  for (let index = 0; index < binary.length; index += 1) {
    certificate[index] = binary.charCodeAt(index);
  }
  return certificate;
}

async function sha256Hex(value: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', value);
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, '0'),
  ).join('');
}

function safeCertificateName(value: string): string {
  return /^[A-Za-z0-9][A-Za-z0-9._-]{0,62}\.cer$/i.test(value)
    ? value
    : 'offline-dental-system-ca.cer';
}

function validServerUrl(value: string): string {
  try {
    const url = new URL(value);
    if (url.protocol !== 'https:' || url.username || url.password) throw new Error();
    return url.href;
  } catch {
    throw pairingError(
      'INVALID_SERVER_URL',
      'O endereço retornado pelo servidor é inválido.',
    );
  }
}

function formatFingerprint(value: string): string {
  return (
    value
      .toUpperCase()
      .match(/.{1,4}/g)
      ?.join(' ') ?? value
  );
}

export function PairingClientScreen({
  material,
  service,
}: {
  material: PairingMaterial | null;
  service: PairingService;
}) {
  const [verification, setVerification] = useState<VerificationState>({
    status: 'verifying',
  });

  useEffect(() => {
    if (!material) return;
    const capturedMaterial = material;
    let active = true;
    let certificateUrl: string | null = null;

    async function completePairing() {
      try {
        const response = await service.complete(capturedMaterial.token);
        const certificate = decodeCertificate(response.caCertificateDerBase64);
        const computedFingerprint = await sha256Hex(certificate);
        const responseFingerprint = response.fingerprintSha256.toLowerCase();

        if (
          !/^[a-f0-9]{64}$/.test(responseFingerprint) ||
          responseFingerprint !== capturedMaterial.expectedFingerprintSha256 ||
          computedFingerprint !== capturedMaterial.expectedFingerprintSha256
        ) {
          throw pairingError(
            'PAIRING_FINGERPRINT_MISMATCH',
            'O fingerprint do certificado não corresponde ao exibido no computador servidor.',
          );
        }

        const certificateFileName = safeCertificateName(response.caFileName);
        const serverUrl = validServerUrl(response.serverUrl);
        certificateUrl = URL.createObjectURL(
          new Blob([certificate], { type: 'application/pkix-cert' }),
        );
        if (!active) {
          URL.revokeObjectURL(certificateUrl);
          return;
        }

        setVerification({
          status: 'verified',
          result: {
            certificateUrl,
            certificateFileName,
            fingerprintSha256: computedFingerprint,
            serverUrl,
          },
        });
      } catch (caught) {
        if (certificateUrl) {
          URL.revokeObjectURL(certificateUrl);
          certificateUrl = null;
        }
        if (active) setVerification({ status: 'error', error: toApiError(caught) });
      }
    }

    void completePairing();
    return () => {
      active = false;
      if (certificateUrl) URL.revokeObjectURL(certificateUrl);
    };
  }, [material, service]);

  if (!material) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-slate-50 px-5 py-10">
        <Card className="w-full max-w-lg p-7 text-center">
          <AppLogo />
          <h1 className="mt-7 text-2xl font-bold text-slate-950">Pareamento inválido</h1>
          <p className="mt-3 text-sm leading-6 text-slate-600">
            O endereço não contém um token e fingerprint válidos. Gere um novo QR code no
            computador servidor.
          </p>
        </Card>
      </main>
    );
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-slate-50 px-5 py-10">
      <div className="w-full max-w-2xl">
        <div className="mb-8 flex justify-center">
          <AppLogo />
        </div>
        <Card className="p-6 sm:p-9">
          <span className="grid size-12 place-items-center rounded-xl bg-petrol-50 text-petrol-700">
            <ShieldCheck className="size-6" aria-hidden="true" />
          </span>
          <p className="mt-6 text-xs font-bold uppercase tracking-[0.16em] text-petrol-700">
            Pareamento local
          </p>
          <h1 className="mt-2 text-3xl font-bold tracking-tight text-slate-950">
            Confirme este dispositivo
          </h1>

          {verification.status === 'verifying' ? (
            <div className="mt-6" role="status">
              <p className="text-sm font-semibold text-slate-700">
                Verificando o certificado e consumindo a autorização de uso único…
              </p>
            </div>
          ) : null}

          {verification.status === 'error' ? (
            <Alert className="mt-6" variant="danger" title="Pareamento recusado">
              <p>{verification.error.message}</p>
              <p className="mt-1 text-xs">
                Referência: {verification.error.correlationId}
              </p>
            </Alert>
          ) : null}

          {verification.status === 'verified' ? (
            <div className="mt-6 grid gap-5">
              <Alert variant="success" title="Certificado verificado localmente">
                O SHA-256 do arquivo recebido corresponde ao fingerprint do QR code e ao
                valor confirmado pelo servidor.
              </Alert>

              <div className="rounded-lg border border-slate-200 p-4">
                <p className="text-xs font-semibold uppercase tracking-wide text-slate-500">
                  Fingerprint SHA-256
                </p>
                <code className="mt-2 block break-all text-xs leading-6 text-slate-950">
                  {formatFingerprint(verification.result.fingerprintSha256)}
                </code>
              </div>

              <Alert variant="warning" title="A confiança é instalada no sistema">
                <span className="inline-flex items-start gap-2">
                  <Fingerprint className="mt-1 size-4 shrink-0" aria-hidden="true" />
                  Baixe o certificado, abra-o pelo sistema operacional e compare novamente
                  o fingerprint antes de confiar na autoridade. Esta página não instala
                  nem aceita certificados automaticamente.
                </span>
              </Alert>

              <div className="grid gap-3 sm:grid-cols-2">
                <a
                  className="inline-flex min-h-11 items-center justify-center gap-2 rounded-lg bg-petrol-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-petrol-800 focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-petrol-500"
                  href={verification.result.certificateUrl}
                  download={verification.result.certificateFileName}
                >
                  <Download className="size-4" aria-hidden="true" />
                  Baixar certificado .cer
                </a>
                <a
                  className="inline-flex min-h-11 items-center justify-center gap-2 rounded-lg border border-slate-300 bg-white px-4 py-2.5 text-sm font-semibold text-slate-800 hover:bg-petrol-50 focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-petrol-500"
                  href={verification.result.serverUrl}
                >
                  Abrir sistema
                  <ExternalLink className="size-4" aria-hidden="true" />
                </a>
              </div>
            </div>
          ) : null}
        </Card>
      </div>
    </main>
  );
}
