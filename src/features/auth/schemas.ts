import { z } from 'zod';

export const loginSchema = z.object({
  username: z
    .string()
    .trim()
    .min(1, 'Informe o nome de usuário.')
    .max(64, 'Use no máximo 64 caracteres.'),
  password: z
    .string()
    .min(1, 'Informe a senha.')
    .refine((value) => Array.from(value).length <= 128, {
      message: 'Use no máximo 128 caracteres.',
    }),
});

export type LoginValues = z.output<typeof loginSchema>;

const authenticatedUserSchema = z.object({
  id: z.string().min(1).max(128),
  fullName: z.string().min(1).max(120),
  username: z.string().min(1).max(64),
  email: z.string().max(254),
  roles: z.array(z.string().min(1).max(64)).max(32),
});

export const authSessionSchema = z.discriminatedUnion('authenticated', [
  z.object({ authenticated: z.literal(false) }),
  z.object({
    authenticated: z.literal(true),
    user: authenticatedUserSchema,
    idleExpiresAt: z.string().refine((value) => !Number.isNaN(Date.parse(value))),
    absoluteExpiresAt: z.string().refine((value) => !Number.isNaN(Date.parse(value))),
  }),
]);
