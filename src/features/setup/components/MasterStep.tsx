import { zodResolver } from '@hookform/resolvers/zod';
import { ArrowLeft, ArrowRight, ShieldCheck } from 'lucide-react';
import { useForm, useWatch } from 'react-hook-form';

import { Alert } from '../../../components/ui/Alert';
import { Button } from '../../../components/ui/Button';
import { Card } from '../../../components/ui/Card';
import { Field } from '../../../components/ui/Field';
import { masterSchema, passwordStrength } from '../schemas';
import type { MasterValues } from '../schemas';
import type { CommandError } from '../types';
import { CommandErrorAlert } from './CommandErrorAlert';
import { StepHeading } from './StepHeading';

interface MasterStepProps {
  initialValues: MasterValues;
  commandError: CommandError | null;
  onBack(): void;
  onContinue(values: MasterValues): void;
}

export function MasterStep({
  initialValues,
  commandError,
  onBack,
  onContinue,
}: MasterStepProps) {
  const {
    register,
    handleSubmit,
    control,
    formState: { errors },
  } = useForm<MasterValues>({
    resolver: zodResolver(masterSchema),
    defaultValues: initialValues,
    mode: 'onBlur',
  });
  const password = useWatch({ control, name: 'password' });
  const strength = passwordStrength(password);

  return (
    <form onSubmit={handleSubmit(onContinue)} noValidate>
      <StepHeading
        eyebrow="Etapa 3 de 5"
        title="Crie o administrador mestre"
        description="Esta conta terá controle integral da instalação. Não existe senha padrão ou recuperação por e-mail."
      />

      <CommandErrorAlert error={commandError} />

      <Card className="p-5 sm:p-7">
        <div className="mb-6 flex items-center gap-3 border-b border-slate-100 pb-4">
          <span className="grid size-10 place-items-center rounded-lg bg-petrol-50 text-petrol-700">
            <ShieldCheck className="size-5" aria-hidden="true" />
          </span>
          <div>
            <h2 className="font-bold text-slate-900">Credenciais locais</h2>
            <p className="text-sm text-slate-500">
              A senha nunca será usada como chave do banco.
            </p>
          </div>
        </div>

        <div className="grid gap-5 sm:grid-cols-2">
          <Field
            label="Nome completo"
            autoComplete="name"
            autoFocus
            error={
              errors.fullName?.message ?? commandError?.fieldErrors?.['master.fullName']
            }
            {...register('fullName')}
          />
          <Field
            label="Nome de usuário"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            hint="3 a 64 caracteres: letras minúsculas, números, ponto, hífen ou sublinhado."
            error={
              errors.username?.message ?? commandError?.fieldErrors?.['master.username']
            }
            {...register('username')}
          />
          <div className="sm:col-span-2">
            <Field
              label="E-mail"
              type="email"
              autoComplete="email"
              error={errors.email?.message ?? commandError?.fieldErrors?.['master.email']}
              {...register('email')}
            />
          </div>
          <Field
            label="Senha"
            type="password"
            autoComplete="new-password"
            hint="Use uma frase longa de 15 a 128 caracteres. Espaços são permitidos."
            error={
              errors.password?.message ?? commandError?.fieldErrors?.['master.password']
            }
            {...register('password')}
          />
          <Field
            label="Confirmar senha"
            type="password"
            autoComplete="new-password"
            error={
              errors.passwordConfirmation?.message ??
              commandError?.fieldErrors?.['master.passwordConfirmation']
            }
            {...register('passwordConfirmation')}
          />
          <div className="sm:col-span-2" aria-live="polite">
            <div className="mb-1.5 flex justify-between text-xs">
              <span className="font-semibold text-slate-700">Qualidade da frase</span>
              <span className="text-slate-500">{strength.label}</span>
            </div>
            <div className="grid grid-cols-4 gap-1" aria-hidden="true">
              {[1, 2, 3, 4].map((part) => (
                <span
                  key={part}
                  className={`h-1.5 rounded-full ${
                    strength.score >= part ? 'bg-petrol-600' : 'bg-slate-200'
                  }`}
                />
              ))}
            </div>
          </div>
        </div>
      </Card>

      <Alert className="mt-4" variant="warning" title="Guarde a senha com cuidado">
        O sistema não enviará links por e-mail. O acesso depende desta senha e dos
        artefatos de recuperação criados na próxima etapa.
      </Alert>

      <div className="mt-6 flex items-center justify-between gap-3">
        <Button
          variant="ghost"
          icon={<ArrowLeft className="size-4" aria-hidden="true" />}
          onClick={onBack}
        >
          Voltar
        </Button>
        <Button type="submit" icon={<ArrowRight className="size-4" aria-hidden="true" />}>
          Continuar
        </Button>
      </div>
    </form>
  );
}
