# ADR 003 — Envelope de recovery e backup inicial

- **Status:** aceita
- **Data:** 2026-07-22

## Contexto

DPAPI é vinculado à conta/máquina e não permite recuperar uma instalação perdida em outro computador. Um backup sem a chave é inútil; guardar a chave junto e desprotegida elimina a proteção. A configuração inicial precisa provar que os artefatos foram realmente criados e podem ser abertos antes de liberar o sistema.

## Decisão

Gerar dois arquivos independentes e exigir um código separado:

### `.odskey`

- código: 36 bytes de CSPRNG, Base64URL sem padding com 48 caracteres;
- KEK: Argon2id v19, `m=65536 KiB`, `t=3`, `p=1`, saída 32 bytes e salt independente de 16 bytes;
- AEAD: XChaCha20-Poly1305, nonce de 24 bytes;
- formato: magic/versão, header JSON em bytes exatos usado como AAD e ciphertext/tag;
- payload: somente `K_db` de 32 bytes; `database_id`/`key_id` ficam no header autenticado como AAD.

### `.odsbackup`

- snapshot consistente pela SQLite Online Backup API, cifrado com a mesma `K_db`;
- magic/versão, manifesto JSON, HMAC-SHA256, comprimento e snapshot;
- chave de HMAC derivada de `K_db` por HKDF com contexto exclusivo;
- o backup não contém `.odskey` nem código.

Os arquivos usam nomes:

```text
offline-dental-recovery-{setupId}-{artifactId}.odskey
offline-dental-backup-{setupId}-{artifactId}.odsbackup
```

São gravados em destinos controlados dentro de volumes locais distintos e graváveis, sempre fora do diretório de dados ativo. O frontend envia somente IDs enumerados pelo serviço. Cada escrita usa `.partial`, flush/sync, reabertura/validação e rename atômico no mesmo volume, sem substituir arquivo existente.

O setup cria organização/master e auditoria, gera/valida `.odskey`, gera/valida o backup inicial e grava a conclusão por último. O usuário redigita o código antes da confirmação. O backend não mantém o código em cache: cada retomada gera novo par/código, e destinos opcionais são revalidados e atualizados em transação auditada para suportar mídia removida.

## Consequências

Positivas:

- perda do DPAPI não impede recovery quando os três materiais são preservados;
- furto de apenas backup, envelope ou código não basta isoladamente;
- corrupção e troca de metadados são detectadas antes de uso;
- o backup inicial cria um baseline portável logo no primeiro acesso.

Custos e limites:

- operador precisa custodiar três materiais em locais separados;
- perda de um item necessário pode tornar recovery impossível;
- cópia antiga de `.odskey` continua válida para bancos com a mesma `K_db`;
- revogação real exige rekey e novos backups;
- restauração completa/rollback ainda não integra esta fundação.

## Alternativas rejeitadas

- **Incluir `.odskey` no `.odsbackup`:** reduz separação e facilita perda conjunta.
- **Guardar código automaticamente:** transforma dois controles em um único artefato.
- **ZIP com senha:** formato/cripto variáveis, sem vínculo forte entre manifesto e banco.
- **Derivar `K_db` da senha do master:** dificulta multiusuário, troca/reset e recuperação.
- **Copiar arquivo SQLite/WAL:** não garante snapshot consistente.

## Critérios de aceite

- código incorreto, header/tag alterado ou arquivos truncados são rejeitados;
- IDs de setup/chave/banco coincidem entre materiais;
- backup inicial abre com SQLCipher e passa integridade/schema/foreign keys;
- código/chaves não aparecem em log, auditoria ou frontend após o fluxo;
- setup incompleto não libera o uso;
- restore em outra máquina será gate separado antes de produção.
