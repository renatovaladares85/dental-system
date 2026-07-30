import { zodResolver } from '@hookform/resolvers/zod';
import { ArrowLeft, ArrowRight, Building2 } from 'lucide-react';
import { useForm } from 'react-hook-form';

import { Button } from '../../../components/ui/Button';
import { Card } from '../../../components/ui/Card';
import { Field } from '../../../components/ui/Field';
import { clinicSchema } from '../schemas';
import type { ClinicFormValues, ClinicValues } from '../schemas';
import type { CommandError } from '../types';
import { CommandErrorAlert } from './CommandErrorAlert';
import { StepHeading } from './StepHeading';

interface ClinicStepProps {
  initialValues: ClinicFormValues;
  commandError: CommandError | null;
  onBack(): void;
  onContinue(values: ClinicValues): void;
}

export function ClinicStep({
  initialValues,
  commandError,
  onBack,
  onContinue,
}: ClinicStepProps) {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<ClinicFormValues, unknown, ClinicValues>({
    resolver: zodResolver(clinicSchema),
    defaultValues: initialValues,
    mode: 'onBlur',
  });

  return (
    <form onSubmit={handleSubmit(onContinue)} noValidate>
      <StepHeading
        eyebrow="Etapa 2 de 5"
        title="Identifique sua clínica"
        description="Esses dados serão usados como referência da instalação e da unidade principal."
      />

      <CommandErrorAlert error={commandError} />

      <Card className="p-5 sm:p-7">
        <div className="mb-6 flex items-center gap-3 border-b border-slate-100 pb-4">
          <span className="grid size-10 place-items-center rounded-lg bg-petrol-50 text-petrol-700">
            <Building2 className="size-5" aria-hidden="true" />
          </span>
          <div>
            <h2 className="font-bold text-slate-900">Clínica e unidade principal</h2>
            <p className="text-sm text-slate-500">
              Você poderá complementar esses dados depois.
            </p>
          </div>
        </div>

        <div className="grid gap-5 sm:grid-cols-2">
          <Field
            label="Nome da clínica"
            autoComplete="organization"
            autoFocus
            error={
              errors.organizationName?.message ??
              commandError?.fieldErrors?.['organization.name']
            }
            {...register('organizationName')}
          />
          <Field
            label="Nome da unidade"
            error={errors.unitName?.message ?? commandError?.fieldErrors?.['unit.name']}
            {...register('unitName')}
          />
          <Field
            label="Responsável pela unidade"
            autoComplete="name"
            error={
              errors.responsibleName?.message ??
              commandError?.fieldErrors?.['unit.responsibleName']
            }
            {...register('responsibleName')}
          />
          <Field
            label="Telefone"
            optional
            autoComplete="tel"
            inputMode="tel"
            error={errors.phone?.message ?? commandError?.fieldErrors?.['unit.phone']}
            {...register('phone')}
          />
          <Field
            label="E-mail administrativo"
            optional
            type="email"
            autoComplete="email"
            error={
              errors.administrativeEmail?.message ??
              commandError?.fieldErrors?.['unit.administrativeEmail']
            }
            {...register('administrativeEmail')}
          />
          <Field
            label="CRO / registro profissional"
            optional
            error={
              errors.professionalRegistration?.message ??
              commandError?.fieldErrors?.['unit.professionalRegistration']
            }
            {...register('professionalRegistration')}
          />
          <div className="sm:col-span-2">
            <Field
              label="Endereço"
              optional
              autoComplete="street-address"
              error={
                errors.address?.message ?? commandError?.fieldErrors?.['unit.address']
              }
              {...register('address')}
            />
          </div>
        </div>
      </Card>

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
