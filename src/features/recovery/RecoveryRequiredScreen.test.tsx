import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { RecoveryRequiredScreen } from './RecoveryRequiredScreen';
import type { RecoveryReasonCode } from '../setup/types';

const expectedMessages: Record<RecoveryReasonCode, string> = {
  missing_key: 'A chave protegida desta instalação não foi encontrada.',
  invalid_key: 'A chave local não pôde ser aberta pela identidade do serviço Windows.',
  database_unreadable: 'A base local não pôde ser aberta ou validada.',
};

describe('RecoveryRequiredScreen', () => {
  it.each(Object.entries(expectedMessages))(
    'bloqueia operações e preserva dados para %s',
    (reasonCode, expectedMessage) => {
      render(<RecoveryRequiredScreen reasonCode={reasonCode as RecoveryReasonCode} />);

      expect(
        screen.getByRole('heading', { name: 'A instalação foi bloqueada com segurança' }),
      ).toBeInTheDocument();
      expect(screen.getByText(expectedMessage, { exact: false })).toBeInTheDocument();
      expect(
        screen.getByText(/Não apague a base, a proteção DPAPI, backups/),
      ).toBeInTheDocument();
      expect(
        screen.getByRole('button', { name: 'Selecionar pacote .odskey' }),
      ).toBeDisabled();
      expect(screen.getByRole('button', { name: 'Iniciar recuperação' })).toBeDisabled();
      expect(
        screen.getByText('Recuperação ainda não disponível nesta entrega.'),
      ).toBeInTheDocument();
    },
  );
});
