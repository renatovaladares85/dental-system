import { z } from 'zod';

import commonPasswordsV1 from '../../../src-tauri/resources/common-passwords-v1.txt?raw';

const optionalText = (maximum: number) =>
  z.string().trim().max(maximum, `Use no máximo ${maximum} caracteres.`);

export const clinicSchema = z.object({
  organizationName: z
    .string()
    .trim()
    .min(2, 'Informe o nome da clínica.')
    .max(120, 'Use no máximo 120 caracteres.'),
  unitName: z
    .string()
    .trim()
    .min(2, 'Informe o nome da unidade.')
    .max(120, 'Use no máximo 120 caracteres.'),
  responsibleName: z
    .string()
    .trim()
    .min(2, 'Informe o responsável pela unidade.')
    .max(120, 'Use no máximo 120 caracteres.'),
  phone: optionalText(32),
  administrativeEmail: z
    .union([z.literal(''), z.email('Informe um e-mail administrativo válido.')])
    .transform((value) => value.trim()),
  address: optionalText(240),
  professionalRegistration: optionalText(40),
});

function codePointLength(value: string): number {
  return Array.from(value).length;
}

function normalizeForComparison(value: string): string {
  return value.normalize('NFC').toLowerCase().trim();
}

const COMMON_PASSWORDS = new Set(
  commonPasswordsV1
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith('#'))
    .map(normalizeForComparison),
);

export const masterSchema = z
  .object({
    fullName: z
      .string()
      .trim()
      .min(2, 'Informe o nome completo.')
      .max(120, 'Use no máximo 120 caracteres.'),
    username: z
      .string()
      .trim()
      .min(3, 'Use pelo menos 3 caracteres.')
      .max(64, 'Use no máximo 64 caracteres.')
      .regex(
        /^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$/,
        'Use letras minúsculas, números, ponto, hífen ou sublinhado.',
      ),
    email: z.email('Informe um e-mail válido.').max(254, 'E-mail muito longo.'),
    password: z.string(),
    passwordConfirmation: z.string(),
  })
  .superRefine((value, context) => {
    const passwordLength = codePointLength(value.password);
    const normalizedPassword = normalizeForComparison(value.password);
    const nameParts = normalizeForComparison(value.fullName)
      .split(/\s+/)
      .filter((part) => part.length >= 3);

    if (passwordLength < 15) {
      context.addIssue({
        code: 'custom',
        path: ['password'],
        message: 'Use pelo menos 15 caracteres.',
      });
    }

    if (passwordLength > 128) {
      context.addIssue({
        code: 'custom',
        path: ['password'],
        message: 'Use no máximo 128 caracteres.',
      });
    }

    if (
      COMMON_PASSWORDS.has(normalizedPassword) ||
      normalizedPassword.includes(normalizeForComparison(value.username)) ||
      nameParts.some((part) => normalizedPassword.includes(part))
    ) {
      context.addIssue({
        code: 'custom',
        path: ['password'],
        message: 'Escolha uma frase que não contenha seu nome, usuário ou senha comum.',
      });
    }

    if (value.password.normalize('NFC') !== value.passwordConfirmation.normalize('NFC')) {
      context.addIssue({
        code: 'custom',
        path: ['passwordConfirmation'],
        message: 'As senhas não coincidem.',
      });
    }
  });

export const confirmationSchema = z.object({
  recoveryCode: z.string().min(1, 'Digite novamente o código de recuperação.'),
  acknowledgedSeparateStorage: z.literal(true, {
    error: 'Confirme que os arquivos serão mantidos separados.',
  }),
  acknowledgedLossRisk: z.literal(true, {
    error: 'Confirme que compreendeu o risco de perda.',
  }),
});

export type ClinicFormValues = z.input<typeof clinicSchema>;
export type ClinicValues = z.output<typeof clinicSchema>;
export type MasterValues = z.output<typeof masterSchema>;
export type ConfirmationValues = z.output<typeof confirmationSchema>;

export function normalizeRecoveryCode(value: string): string {
  return value.replace(/\s/g, '');
}

export function passwordStrength(value: string): {
  score: number;
  label: string;
} {
  const length = codePointLength(value);
  let score = 0;

  if (length >= 15) score += 1;
  if (length >= 20) score += 1;
  if (/\s/.test(value)) score += 1;
  if (new Set(Array.from(value)).size >= 10) score += 1;

  if (score <= 1) return { score, label: 'Insuficiente' };
  if (score === 2) return { score, label: 'Razoável' };
  if (score === 3) return { score, label: 'Boa' };
  return { score, label: 'Forte' };
}
