import { zodResolver } from '@hookform/resolvers/zod';
import { LockKeyhole, ShieldCheck, UserRound, WifiOff } from 'lucide-react';
import { useState } from 'react';
import { useForm } from 'react-hook-form';

import { AppLogo } from '../../components/brand/AppLogo';
import { Alert } from '../../components/ui/Alert';
import { Button } from '../../components/ui/Button';
import { Card } from '../../components/ui/Card';
import { Field } from '../../components/ui/Field';
import { toApiError } from '../../lib/api';
import type { SecurityDiagnostics } from '../setup/types';
import { loginSchema } from './schemas';
import type { LoginValues } from './schemas';
import type { AuthService, AuthenticatedSession } from './types';

interface LoginScreenProps {
  service: AuthService;
  diagnostics?: SecurityDiagnostics;
  onAuthenticated(session: AuthenticatedSession): void;
  onServerUnavailable(): void;
}

export function LoginScreen({
  service,
  diagnostics,
  onAuthenticated,
  onServerUnavailable,
}: LoginScreenProps) {
  const [error, setError] = useState<ReturnType<typeof toApiError> | null>(null);
  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm<LoginValues>({
    resolver: zodResolver(loginSchema),
    defaultValues: { username: '', password: '' },
  });

  async function submit(values: LoginValues) {
    setError(null);
    try {
      onAuthenticated(
        await service.login({
          username: values.username.trim(),
          password: values.password,
        }),
      );
    } catch (caught) {
      const commandError = toApiError(caught);
      if (commandError.code === 'SERVER_UNAVAILABLE') {
        onServerUnavailable();
        return;
      }
      setError(commandError);
    }
  }

  return (
    <main className="grid min-h-screen bg-slate-50 lg:grid-cols-[minmax(25rem,42%)_1fr]">
      <section className="relative hidden overflow-hidden bg-petrol-950 p-12 text-white lg:flex lg:flex-col">
        <div className="relative z-10">
          <AppLogo inverted />
        </div>
        <div className="relative z-10 my-auto max-w-lg">
          <span className="mb-6 grid size-14 place-items-center rounded-xl bg-white/10 text-petrol-200">
            <ShieldCheck className="size-8" strokeWidth={1.5} aria-hidden="true" />
          </span>
          <h1 className="text-4xl font-bold tracking-tight">
            Gestão clínica na rede local.
          </h1>
          <p className="mt-5 text-base leading-8 text-slate-300">
            Acesse o servidor da clínica com uma sessão protegida. A aplicação continua
            independente da internet, sem copiar dados clínicos para este dispositivo.
          </p>
        </div>
        <p className="relative z-10 text-xs text-slate-400">
          Offline Dental System · servidor local
        </p>
        <div className="absolute -bottom-36 -right-36 size-[30rem] rounded-full border border-white/5 bg-petrol-800/30" />
        <div className="absolute -bottom-16 -right-16 size-64 rounded-full border border-white/5" />
      </section>

      <section className="flex items-center justify-center px-5 py-10 sm:px-10">
        <div className="w-full max-w-md">
          <div className="mb-8 lg:hidden">
            <AppLogo />
          </div>
          <p className="text-xs font-bold uppercase tracking-[0.16em] text-petrol-700">
            Ambiente da clínica
          </p>
          <h2 className="mt-2 text-3xl font-bold tracking-tight text-slate-950">
            Acesse sua conta
          </h2>
          <p className="mt-3 text-sm leading-6 text-slate-600">
            Use o administrador mestre criado durante a configuração inicial.
          </p>

          <Card className="mt-7 p-6">
            <form
              className="grid gap-5"
              onSubmit={(event) => void handleSubmit(submit)(event)}
            >
              <Field
                label="Nome de usuário"
                autoComplete="username"
                autoCapitalize="none"
                spellCheck={false}
                error={errors.username?.message}
                autoFocus
                trailing={
                  <UserRound
                    className="absolute right-3 top-3.5 size-4 text-slate-400"
                    aria-hidden="true"
                  />
                }
                {...register('username')}
              />
              <Field
                label="Senha"
                type="password"
                autoComplete="current-password"
                error={errors.password?.message}
                {...register('password')}
              />

              {error ? (
                <Alert variant="danger" title="Não foi possível entrar">
                  <p>{error.message}</p>
                  <p className="mt-1 text-xs">Referência: {error.correlationId}</p>
                </Alert>
              ) : null}

              <Button
                type="submit"
                className="w-full"
                busy={isSubmitting}
                icon={<LockKeyhole className="size-4" aria-hidden="true" />}
              >
                Entrar
              </Button>
            </form>
          </Card>

          <Alert className="mt-4" title="Uso local, sem cache clínico" variant="info">
            <span className="inline-flex items-start gap-2">
              <WifiOff className="mt-1 size-4 shrink-0" aria-hidden="true" />
              Sem acesso ao servidor, apenas esta interface permanece disponível; dados e
              operações não são armazenados no navegador.
            </span>
          </Alert>

          {diagnostics && !diagnostics.distributionReady ? (
            <Alert className="mt-4" variant="warning" title="Build de desenvolvimento">
              SQLCipher {diagnostics.sqlcipherVersion ?? 'não detectado'}. A distribuição
              exige a versão {diagnostics.minimumDistributionVersion} ou superior.
            </Alert>
          ) : null}
        </div>
      </section>
    </main>
  );
}
