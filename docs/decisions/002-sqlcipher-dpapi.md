# ADR 002 — SQLCipher e DPAPI

- **Status:** aceita, com gate de produção pendente
- **Data:** 2026-07-22

## Contexto

O banco local contém dados sensíveis e deve permanecer portátil por backup, mas a instalação precisa abrir sem depender de nuvem nem pedir uma segunda senha de vault antes do login interno. Criptografia por campo deixaria schema/índices/metadados expostos e criaria criptografia customizada difícil de manter.

## Decisão

Usar SQLCipher para cifrar o banco inteiro e uma chave de dados aleatória `K_db` de 32 bytes. Na instalação ativa, `K_db` é protegida pelo Windows DPAPI no escopo do usuário atual.

- CSPRNG do sistema gera `K_db`; senha humana não gera a chave do banco.
- `CryptProtectData` usa `CRYPTPROTECT_UI_FORBIDDEN`, sem escopo de máquina.
- O blob DPAPI reside em `%ProgramData%\OfflineDentalSystem`, sob ACL do serviço, e se vincula a `database_id`/`key_id`.
- A chave é aplicada antes de qualquer consulta; uma leitura do schema confirma chave correta.
- `cipher_status`, `cipher_integrity_check`, `integrity_check` e `foreign_key_check` integram diagnóstico/fluxos críticos.
- Temporários ficam em memória; extensão dinâmica e fallback SQLite em claro são proibidos.
- `cipher_memory_security` permanece desligado até o bundle 4.17 passar pelo teste específico de falha de quota do `VirtualLock`; aprovação da versão, isoladamente, não basta.
- SQL/chave não entram em trace, log ou HTTP.

## Gate de versão

Produção exige SQLCipher **≥ 4.17.0**. Como a release publicada de `libsqlite3-sys 0.38.1` ainda incorporava 4.14.0, o projeto fixa uma revisão exata do repositório oficial `rusqlite` cujo amalgamation declara 4.17.0. O `Cargo.lock` fixa o grafo e o diagnóstico confirma a versão real por `PRAGMA cipher_version`; nenhum metadado de dependência substitui essa verificação.

Atualizar a dependência não basta: CI deve confirmar `PRAGMA cipher_version`, banco/WAL sem cabeçalho ou marcador em claro, chave incorreta rejeitada e testes de backup/recovery no Windows alvo.

## Consequências

Positivas:

- páginas, índices, WAL e journal são protegidos por uma implementação consolidada;
- DPAPI reduz armazenamento manual de segredo e funciona offline;
- login interno e rotação de senha não exigem rekey do banco.

Limites:

- processo malicioso no mesmo usuário Windows pode acionar DPAPI/inspecionar memória;
- redefinição administrativa da senha Windows pode perder acesso ao blob;
- mesma base não é compartilhada automaticamente entre contas Windows;
- SQLCipher/OpenSSL aumenta custo de build e atribuições de licença.

O pacote de recovery separado é obrigatório para mitigar perda da máquina/conta; ver [ADR 003](003-recovery-envelope-initial-backup.md).

## Alternativas consideradas

- **Vault adicional/Stronghold:** rejeitado nesta topologia. Exigiria proteger outro segredo sem melhorar a fronteira do serviço apenas Windows.
- **Credential Manager:** adiado. Tem fronteira equivalente ao usuário e ciclo de vida externo com risco de entrada órfã.
- **DPAPI no escopo da máquina:** rejeitado; outros usuários do computador poderiam abrir o material.
- **SQLite em claro + campos cifrados:** rejeitado; vaza metadados e cria alto risco de cobertura parcial.
- **SQLite SEE:** alternativa comercial possível, mas não necessária nesta fundação.

## Referências

- [SQLCipher key material](https://www.zetetic.net/sqlcipher/database-key-material/)
- [SQLCipher API](https://www.zetetic.net/sqlcipher/sqlcipher-api/)
- [SQLCipher 4.17.0](https://www.zetetic.net/blog/2026/07/08/sqlcipher-4.17.0-release/)
- [Microsoft CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
