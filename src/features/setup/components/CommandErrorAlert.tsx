import { Alert } from '../../../components/ui/Alert';
import type { CommandError } from '../types';

export function CommandErrorAlert({
  error,
  title = 'Revise os dados informados',
}: {
  error: CommandError | null;
  title?: string;
}) {
  if (!error) return null;

  return (
    <Alert className="mb-4" variant="danger" title={title}>
      <p>{error.message}</p>
      <p className="mt-1 text-xs">Referência: {error.correlationId}</p>
    </Alert>
  );
}
