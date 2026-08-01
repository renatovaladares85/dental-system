# Logs operacionais e auditoria

## Separação de responsabilidades

O sistema mantém dois registros complementares:

- **log operacional JSONL**: diagnóstico técnico da instalação, desinstalação, serviço e API;
- **auditoria no SQLCipher**: ações autoritativas de negócio e segurança, vinculadas ao usuário e à sessão quando aplicável.

Cliques, digitação, senhas, tokens, CSRF, códigos de recuperação, corpos HTTP, parâmetros de URL e conteúdo clínico não são registrados. A interface consulta a auditoria apenas quando um `MASTER_ADMIN` solicita; não existe telemetria remota.

## Localização

| Registro    | Caminho                                                  | Retenção                                                               |
| ----------- | -------------------------------------------------------- | ---------------------------------------------------------------------- |
| Serviço/API | `%ProgramData%\OfflineDentalSystem\logs\runtime\*.jsonl` | rotação diária, até 30 arquivos                                        |
| Auditoria   | tabela cifrada `audit_events`                            | append-only; política definitiva será definida antes de dados clínicos |

O instalador exibe o caminho exato do log em caso de sucesso ou falha. A execução normal e a execução elevada compartilham o mesmo `runId`, portanto o erro administrativo não desaparece quando a janela do UAC fecha.

## Campos operacionais

Cada linha é um objeto JSON independente. Os eventos de runtime contêm `runId`,
versão, modo, PID quando disponível, fase, código e resultado. Os eventos HTTP
contêm somente método, modelo de rota, status, duração, IP remoto e
`correlation_id`.

O diagnóstico de serviço usa exit code e configuração persistida; nunca depende
de texto localizado do Windows.

O servidor devolve `X-Correlation-ID` nas respostas. Essa referência permite correlacionar uma mensagem segura da interface com o log técnico sem expor a causa interna ao navegador.

## Eventos auditados nesta fundação

- criação e retomada do setup e confirmação dos artefatos;
- login e logout do administrador mestre;
- rotação de segurança da sessão;
- consulta do próprio histórico de auditoria.

Cada novo caso de uso autoritativo deve adicionar seu evento à mesma transação que altera o estado. A auditoria possui triggers que recusam `UPDATE` e `DELETE`. Falhas HTTP sem usuário autenticado permanecem no log operacional com status, IP e correlação, sem persistir o nome de usuário apresentado.

## Diagnóstico de instalação

1. Execute `scripts/diagnostics/inspect-installation.ps1`.
2. Execute `offline-dental-system.exe --startup-diagnostics --json` quando o binário estiver disponível; o modo é estritamente somente leitura.
3. Identifique o último evento `STARTUP_FAILED` e seu código sanitizado.
4. Preserve banco, DPAPI, backups e logs antes de qualquer correção.
5. Não envie o banco, arquivos `.odskey` ou `.odsbackup` como log de suporte.

O rollback do MSI remove somente componentes criados pela instalação atual.
Dados já existentes em `%ProgramData%\OfflineDentalSystem` não são apagados automaticamente.
