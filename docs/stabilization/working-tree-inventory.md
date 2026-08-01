# Inventário do working tree — estabilização

Data da inspeção: 2026-08-01. Este documento contém apenas caminhos e
classificações; não contém hashes de dados, conteúdo clínico, chaves,
certificados ou logs.

| Caminho                                            | Classificação                             | Tratamento nesta estabilização                                         |
| -------------------------------------------------- | ----------------------------------------- | ---------------------------------------------------------------------- |
| `.local-data/`                                     | dado local / material sensível            | Preservar integralmente; nunca limpar por padrão.                      |
| `artifacts/portable/`                              | instalador experimental / artefato gerado | Preservar fora do fluxo suportado; não publicar nem executar.          |
| `dist/`                                            | artefato gerado                           | Pode ser recriado pelo build; não foi removido nesta fase.             |
| `node_modules/`                                    | artefato gerado                           | Pode ser recriado por `npm ci`; não foi removido nesta fase.           |
| `src-tauri/target/`                                | artefato gerado                           | Pode ser recriado pelo Cargo; não foi removido nesta fase.             |
| `src-tauri/gen/schemas/`                           | artefato gerado                           | Mantido ignorado; não há gerador invocado nesta estabilização.         |
| `installer/validation-NOT-FOR-DISTRIBUTION.txt`    | fixture local de validação                | Removido: `ValidationOnly` agora cria e apaga fixture temporário.      |
| `src/features/recovery/RecoveryRequiredScreen.tsx` | código-fonte necessário                   | Recuperado para controle de versão ao remover a regra de ignore ampla. |

## Proteções observadas

- `git clean -ndX` indicou diretórios ignorados sob `src-tauri/`; nenhum comando
  de limpeza será usado como parte deste trabalho.
- Qualquer remoção futura de `.local-data/` será exclusiva de dados de
  desenvolvimento, mediante confirmação explícita ou `-Force` no script próprio.
