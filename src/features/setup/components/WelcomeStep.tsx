import { ArrowRight, DatabaseZap, FileKey2, HardDrive, WifiOff } from 'lucide-react';
import { useState } from 'react';

import { Alert } from '../../../components/ui/Alert';
import { Button } from '../../../components/ui/Button';
import { Card } from '../../../components/ui/Card';
import { StepHeading } from './StepHeading';

interface WelcomeStepProps {
  onStart(): void;
}

const benefits = [
  {
    icon: <WifiOff className="size-5" aria-hidden="true" />,
    title: 'Funciona sem internet',
    description: 'Atendimentos continuam disponíveis mesmo sem conexão.',
  },
  {
    icon: <DatabaseZap className="size-5" aria-hidden="true" />,
    title: 'Dados neste computador',
    description: 'Sem sincronização automática ou servidor externo.',
  },
  {
    icon: <HardDrive className="size-5" aria-hidden="true" />,
    title: 'Backup sob seu controle',
    description: 'Você escolhe onde manter as cópias protegidas.',
  },
];

export function WelcomeStep({ onStart }: WelcomeStepProps) {
  const [showRecoveryNotice, setShowRecoveryNotice] = useState(false);

  return (
    <div>
      <StepHeading
        eyebrow="Configuração inicial"
        title="Prepare seu ambiente clínico"
        description="Em poucos passos, vamos cadastrar a clínica, criar o administrador mestre e proteger a base local."
      />

      <div className="grid gap-4 sm:grid-cols-3">
        {benefits.map((benefit) => (
          <Card key={benefit.title} className="p-5">
            <span className="mb-4 grid size-10 place-items-center rounded-lg bg-petrol-50 text-petrol-700">
              {benefit.icon}
            </span>
            <h2 className="text-sm font-bold text-slate-900">{benefit.title}</h2>
            <p className="mt-1 text-sm leading-6 text-slate-600">{benefit.description}</p>
          </Card>
        ))}
      </div>

      {showRecoveryNotice ? (
        <Alert
          className="mt-5"
          variant="warning"
          title="Restauração ainda não disponível"
        >
          Esta entrega prepara o formato seguro dos artefatos. A restauração completa será
          adicionada na próxima fase, sem tentativa parcial ou simulação de sucesso.
        </Alert>
      ) : null}

      <div className="mt-8 flex flex-col-reverse gap-3 border-t border-slate-200 pt-6 sm:flex-row sm:items-center sm:justify-between">
        <Button
          variant="ghost"
          icon={<FileKey2 className="size-4" aria-hidden="true" />}
          onClick={() => setShowRecoveryNotice(true)}
        >
          Tenho um pacote de recuperação
        </Button>
        <Button
          icon={<ArrowRight className="size-4" aria-hidden="true" />}
          onClick={onStart}
        >
          Configurar novo sistema
        </Button>
      </div>
    </div>
  );
}
