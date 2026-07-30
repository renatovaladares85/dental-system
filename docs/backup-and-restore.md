# Backup, recovery e restauração

## Estado funcional

| Capacidade                                        | Estado desta fundação          |
| ------------------------------------------------- | ------------------------------ |
| gerar e validar pacote inicial `.odskey`          | implementado no fluxo de setup |
| gerar e validar backup inicial `.odsbackup`       | implementado no fluxo de setup |
| abrir diagnóstico sem expor segredos              | implementado na fundação       |
| backup manual/automático recorrente e retenção    | futuro                         |
| restauração completa com troca atômica e rollback | **não implementado**           |
| rotação/revogação operacional de recovery         | futuro                         |

Não use a existência de um arquivo como prova de recuperabilidade. Produção exige exercício de restauração em uma máquina limpa.

## Materiais independentes

Uma recuperação portátil requer três itens:

1. backup `.odsbackup`;
2. pacote `.odskey` correspondente ao `key_id`;
3. código de recuperação de 48 caracteres.

O `.odsbackup` **não contém** o `.odskey`, e o código não é salvo por nenhum deles. Backup e recovery são gravados em **volumes locais distintos** e graváveis, sempre fora do diretório de dados ativo; o código deve ficar em um terceiro local controlado. O navegador envia apenas IDs de volumes enumerados pelo serviço, nunca paths arbitrários. UNC, SMB, reparse point inesperado, destino dentro dos dados ativos e volumes iguais são rejeitados. O blob DPAPI local não é portátil e não substitui o pacote.

## Pacote `.odskey`

Formato binário versionado:

```text
magic | versão/formato | tamanho do header | header JSON exato | ciphertext+tag
```

O header contém somente metadados necessários, como versão, `database_id`, `key_id`, data, Argon2id/salt e XChaCha20-Poly1305/nonce. Os bytes exatos do header são AAD; alterá-los invalida a autenticação. O ciphertext contém apenas os 32 bytes de `K_db`; os identificadores permanecem visíveis, porém autenticados pelo AAD.

Fluxo:

1. gerar código aleatório de 36 bytes e exibir como Base64URL de 48 caracteres;
2. derivar KEK com Argon2id e salt independente;
3. cifrar somente `K_db`; `database_id`/`key_id` ficam no header autenticado;
4. zeroizar buffers best-effort;
5. gravar como `.partial`, sincronizar, reabrir e validar com o código;
6. renomear atomicamente para `.odskey`.

O código é retornado somente pela operação que gera o par, não fica cacheado no backend e deve permanecer apenas no estado transitório da tela. Captura de tela, clipboard persistente e logs devem ser evitados. A confirmação exige redigitação e abertura real do arquivo recém-gravado.

## Backup `.odsbackup`

Formato binário versionado:

```text
magic | versão/formato | tamanho do manifesto | manifesto JSON |
HMAC-SHA256 | tamanho do snapshot | snapshot SQLCipher
```

O manifesto inclui apenas:

- versão do formato, aplicação e schema;
- `setup_id` e `database_id`;
- data UTC;
- versão/configuração SQLCipher relevante;
- tamanho e checksum do snapshot.

A subchave do HMAC é derivada de `K_db` por HKDF com contexto exclusivo. O snapshot continua cifrado pelo SQLCipher. Anexos só podem integrar o snapshot cifrado ou um formato AEAD futuro explicitamente autenticado; nunca entram em claro no arquivo.

O snapshot é produzido pela [SQLite Online Backup API](https://www.sqlite.org/backup.html), com journal `DELETE` no arquivo de destino, não por cópia do arquivo vivo/WAL. O processo grava em staging, valida estrutura/HMAC/abertura SQLCipher e só então renomeia para o destino final. Nomes usados, sem PII:

```text
offline-dental-recovery-{setupId}-{artifactId}.odskey
offline-dental-backup-{setupId}-{artifactId}.odsbackup
```

Nunca sobrescrever silenciosamente um backup anterior.

## Configuração inicial

O setup segue uma máquina de estados recuperável:

1. validar organização, unidade e master;
2. gerar `K_db` e inicializar SQLCipher/migrations;
3. persistir o blob DPAPI no usuário Windows atual;
4. criar organização, unidade, master e auditoria em transação;
5. gerar `.odskey` e mostrar o código;
6. exigir redigitação e validar o envelope;
7. gerar o `.odsbackup` após a transação inicial;
8. validar formato, HMAC, SQLCipher, schema e integridade;
9. pedir confirmação de cópia separada;
10. alterar `installations.setup_state` para `READY` por último, na transação de confirmação.

Falha em qualquer etapa mantém o setup incompleto. Artefatos `.partial` podem ser removidos após identificação segura; um `.odskey` ou `.odsbackup` potencialmente válido não deve ser apagado automaticamente sem confirmação.

### Retomada interrompida

`resume_initial_setup` sempre gera novo `.odskey`, `.odsbackup` e código one-shot; não reutiliza código em memória. `installation_settings` persiste em uma única transação os IDs do par vigente, sem depender da ordenação por timestamp. Pares anteriores permanecem no histórico, não confirmados, e não devem ser confundidos com o vigente.

O endpoint de retomada exige que o operador selecione novamente os dois IDs de volume. O serviço reenumera, resolve destinos controlados e rejeita IDs iguais, volumes de rede/somente leitura ou destino dentro do diretório de dados antes de gerar um novo par. Isso evita confiar em mídia removível ausente ou trocada e permite mudar os destinos sem recriar organização, master ou banco.

## Restauração alvo

O fluxo abaixo é contrato futuro e não está disponível na UI atual:

1. selecionar `.odsbackup` e `.odskey` por diálogo nativo;
2. copiar entradas para staging sob diretório controlado;
3. validar magic, versão, comprimentos e compatibilidade antes de processar conteúdo;
4. solicitar o código e abrir `.odskey` com Argon2id + AEAD;
5. conferir `database_id` entre envelope e manifesto e autenticar o backup com a chave recuperada; `key_id` permanece no envelope/histórico da instalação;
6. validar HMAC/checksum e abrir snapshot somente leitura;
7. executar `cipher_status`, `cipher_integrity_check`, `integrity_check` e `foreign_key_check`;
8. recusar schema mais novo que a aplicação; migrar somente versões suportadas;
9. criar e validar backup de segurança do estado atual;
10. obter confirmação final e encerrar todas as conexões;
11. preparar o novo banco no mesmo volume e trocar atomicamente;
12. reabrir e executar smoke test; em falha, recolocar o estado anterior;
13. somente após sucesso, proteger `K_db` com DPAPI da nova conta Windows;
14. registrar resultado sem segredos e reiniciar de forma controlada.

Nenhum passo pode destruir a base atual antes da validação integral do substituto.

## Compatibilidade

Restaurar é permitido quando:

- magic e versão de container são suportados;
- `key_id` e `database_id` são coerentes;
- HMAC e checksums são válidos;
- SQLCipher atende à versão mínima suportada;
- schema é igual ou migrável pela aplicação instalada;
- tamanho está dentro dos limites e há espaço para staging + backup de segurança + destino.

Importar banco SQLite em claro, arquivo avulso ou ZIP genérico é proibido. Mudança de chave/configuração SQLCipher usa export/rekey controlado, não cópia de bytes presumida.

## Retenção futura

- backup inicial é imutável e não entra em limpeza automática;
- política recorrente deve ser configurável por quantidade e idade;
- limpeza só remove arquivos que passaram por parsing e pertencem ao diretório autorizado;
- manter ao menos uma geração fora do computador;
- alertar sobre último backup válido, não apenas último arquivo criado;
- package/recovery antigo continua capaz de abrir backups daquela chave; invalidá-lo requer rekey.

## Procedimento operacional

Após o setup:

1. copie `.odskey` para mídia segura fora do computador;
2. guarde o código em outro local físico/gerenciador apropriado;
3. copie `.odsbackup` para mídia distinta;
4. registre quem custodia cada material, sem registrar o código;
5. não renomeie conteúdo interno nem edite os arquivos;
6. antes de uso real, execute recovery completo em máquina de teste quando o restore estiver implementado.

Em incidente, preserve todos os arquivos, não reinstale sobre o diretório de dados e trabalhe em cópias. Uma restauração malsucedida não autoriza apagar o estado anterior.

## Testes obrigatórios antes de produção

- round-trip `.odskey` e rejeição de código/header/tag alterados;
- truncamento e comprimentos maliciosos sem alocação excessiva;
- HMAC/checksum inválidos e `key_id` divergente;
- backup durante WAL/escritas concorrentes;
- banco com bit-flip e chave incorreta;
- schema incompatível;
- destino sem espaço/permissão e mídia removida;
- interrupção antes/durante/depois da troca atômica;
- recuperação em outra conta/máquina e novo DPAPI;
- prova de preservação do banco anterior em toda falha;
- ausência de senha, código e chave em logs.
