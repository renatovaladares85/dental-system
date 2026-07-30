import { describe, expect, it } from 'vitest';

import { masterSchema, normalizeRecoveryCode, passwordStrength } from './schemas';

const validMaster = {
  fullName: 'Marina Costa',
  username: 'marina.admin',
  email: 'marina@example.test',
  password: 'Horizonte sereno depois da chuva',
  passwordConfirmation: 'Horizonte sereno depois da chuva',
};

describe('política de credenciais', () => {
  it('aceita uma frase longa sem exigir composição arbitrária', () => {
    expect(masterSchema.safeParse(validMaster).success).toBe(true);
  });

  it('compara a confirmação em NFC sem transformar a senha informada', () => {
    const password = 'segredo longo com cafe\u0301 azul';
    const result = masterSchema.safeParse({
      ...validMaster,
      password,
      passwordConfirmation: 'segredo longo com café azul',
    });

    expect(result.success).toBe(true);
    if (result.success) expect(result.data.password).toBe(password);
  });

  it('rejeita senha que contém o usuário', () => {
    const result = masterSchema.safeParse({
      ...validMaster,
      password: 'frase marina.admin para acesso',
      passwordConfirmation: 'frase marina.admin para acesso',
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.flatten().fieldErrors.password).toContain(
        'Escolha uma frase que não contenha seu nome, usuário ou senha comum.',
      );
    }
  });

  it('rejeita uma entrada carregada da blocklist compartilhada', () => {
    const result = masterSchema.safeParse({
      ...validMaster,
      password: 'odontologia2026',
      passwordConfirmation: 'odontologia2026',
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.flatten().fieldErrors.password).toContain(
        'Escolha uma frase que não contenha seu nome, usuário ou senha comum.',
      );
    }
  });

  it('conta caracteres Unicode e normaliza o código de recuperação', () => {
    expect(
      passwordStrength('odontologia segura 🌱 por muitos anos').score,
    ).toBeGreaterThan(1);
    expect(normalizeRecoveryCode('abcd-efgh ijkl')).toBe('abcd-efghijkl');
  });

  it('preserva hífen Base64URL e remove somente espaços de agrupamento', () => {
    const raw = 'ABCDEF-GHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstu';
    const grouped = raw.match(/.{1,6}/g)?.join(' ') ?? raw;

    expect(raw).toHaveLength(48);
    expect(normalizeRecoveryCode(grouped)).toBe(raw);
  });
});
