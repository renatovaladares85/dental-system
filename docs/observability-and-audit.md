# Logs operacionais e auditoria

## Separação de responsabilidades

O sistema mantém dois registros complementares:

- **log operacional JSONL**: diagnóstico técnico da instalação, desinstalação, serviço e API;
- **auditoria no SQLCipher**: ações autoritativas de negócio e segurança, vinculadas ao usuário e à sessão quando aplicável.

Cliques, digitação, senhas, tokens, CSRF, códigos de recuperação, corpos HTTP, parâmetros de URL e conteúdo clínico não são registrados. A interface consulta a auditoria apenas quando um `MASTER_ADMIN` solicita; não existe telemetria remota.

## Localização

| Registro                 | Caminho                                                        | Retenção                                                               |
| ------------------------ | -------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Instalação/desinstalação | `%LOCALAPPDATA%\OfflineDentalSystem\Logs\installation\*.jsonl` | preservado para suporte local                                          |
| Atalho de abertura       | `%LOCALAPPDATA%\OfflineDentalSystem\Logs\operation\*.jsonl`    | preservado para suporte local                                          |
| Serviço/API              | `%ProgramData%\OfflineDentalSystem\Logs\server.*.jsonl`        | rotação diária, até 30 arquivos                                        |
| Auditoria                | tabela cifrada `audit_events`                                  | append-only; política definitiva será definida antes de dados clínicos |

O instalador exibe o caminho exato do log em caso de sucesso ou falha. A execução normal e a execução elevada compartilham o mesmo `runId`, portanto o erro administrativo não desaparece quando a janela do UAC fecha.

## Campos operacionais

Cada linha é um objeto JSON independente. Os eventos de instalação contêm `timestampUtc`, `runId`, `processId`, `elevated`, `level`, `event`, `message` e `data`. Os eventos HTTP contêm somente método, modelo de rota, status, duração, IP remoto e `correlation_id`.

O evento `SERVICE_CONTROL_COMMAND` registra operação, nome do serviço,
quantidade de argumentos, executável controlado, argumento `--service`, conta,
display name, modo de início, executável nativo e exit code. A saída localizada
do Windows aparece apenas em falhas, limitada e decodificada pela página OEM;
a lógica usa o exit code e a configuração persistida, nunca o texto localizado.

O servidor devolve `X-Correlation-ID` nas respostas. Essa referência permite correlacionar uma mensagem segura da interface com o log técnico sem expor a causa interna ao navegador.

## Eventos auditados nesta fundação

- criação e retomada do setup e confirmação dos artefatos;
- login e logout do administrador mestre;
- rotação de segurança da sessão;
- consulta do próprio histórico de auditoria.

Cada novo caso de uso autoritativo deve adicionar seu evento à mesma transação que altera o estado. A auditoria possui triggers que recusam `UPDATE` e `DELETE`. Falhas HTTP sem usuário autenticado permanecem no log operacional com status, IP e correlação, sem persistir o nome de usuário apresentado.

## Diagnóstico de instalação

1. Execute `Instalar-e-Iniciar.bat` novamente.
2. Copie a mensagem depois de `ERRO:` e o caminho mostrado depois de `Log:`.
3. Abra o JSONL em um editor de texto e procure o último registro com `"level":"ERROR"`.
4. Não envie o banco, arquivos `.odskey` ou `.odsbackup` como log de suporte.

O rollback remove somente a versão que falhou, serviço e regras recém-criados. Dados já existentes em `%ProgramData%\OfflineDentalSystem` não são apagados automaticamente.
