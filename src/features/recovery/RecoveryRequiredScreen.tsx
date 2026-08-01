import { DatabaseZap, FileKey2, ShieldAlert } from 'lucide-react';

import { AppLogo } from '../../components/brand/AppLogo';
import { Alert } from '../../components/ui/Alert';
import { Button } from '../../components/ui/Button';
import { Card } from '../../components/ui/Card';
import type { RecoveryReasonCode } from '../setup/types';

const messages: Record<RecoveryReasonCode, string> = {
  missing_key: 'A chave protegida desta instalação não foi encontrada.',
  invalid_key: 'A chave local não pôde ser aberta pela identidade do serviço Windows.',
  database_unreadable: 'A base local não pôde ser aberta ou validada.',
};

export function RecoveryRequiredScreen({
  reasonCode,
}: {
  reasonCode: RecoveryReasonCode;
}) {
  return (
    <main className="flex min-h-screen items-center justify-center bg-slate-50 px-5 py-10">
      <div className="w-full max-w-2xl">
        <div className="mb-8 flex justify-center">
          <AppLogo />
        </div>
        <Card className="p-6 text-center sm:p-10">
          <span className="mx-auto grid size-14 place-items-center rounded-xl bg-red-50 text-red-700">
            <ShieldAlert className="size-7" aria-hidden="true" />
          </span>
          <p className="mt-6 text-xs font-bold uppercase tracking-[0.16em] text-red-700">
            Recuperação necessária
          </p>
          <h1 className="mt-2 text-3xl font-bold tracking-tight text-slate-950">
            A instalação foi bloqueada com segurança
          </h1>
          <p className="mx-auto mt-4 max-w-lg text-base leading-7 text-slate-600">
            {messages[reasonCode]} Nenhuma tentativa de recriar ou substituir arquivos
            será feita automaticamente.
          </p>

          <Alert
            className="mt-7 text-left"
            variant="warning"
            title="Preserve os arquivos atuais"
          >
            Não apague a base, a proteção DPAPI, backups ou pacotes de recuperação. A
            restauração completa será entregue em uma fase posterior.
          </Alert>

          <div className="mt-7 grid gap-3 sm:grid-cols-2">
            <Button
              variant="secondary"
              disabled
              icon={<FileKey2 className="size-4" aria-hidden="true" />}
            >
              Selecionar pacote .odskey
            </Button>
            <Button disabled icon={<DatabaseZap className="size-4" aria-hidden="true" />}>
              Iniciar recuperação
            </Button>
          </div>
          <p className="mt-3 text-xs text-slate-500">
            Recuperação ainda não disponível nesta entrega.
          </p>
        </Card>
      </div>
    </main>
  );
}
