# Modelo de ameaças e segurança

## Escopo

O sistema processará dados pessoais e clínicos sensíveis em um servidor Windows 11 e em navegadores na mesma LAN. Esta fundação não declara conformidade automática com LGPD, CFO ou política de retenção da clínica, e ainda não está autorizada para dados reais.

## Ativos e fronteiras

Ativos críticos:

- banco SQLCipher, WAL e anexos futuros;
- chave aleatória do banco (`K_db`) e chaves TLS;
- hashes de senha, sessão e CSRF;
- `.odskey`, código de recuperação e `.odsbackup`;
- trilha de auditoria.

Fronteiras de confiança:

1. navegador, Service Worker e LAN são não confiáveis;
2. handlers HTTP validam transporte, mas casos de uso Rust autorizam a operação;
3. SQLCipher e filesystem são acessíveis somente pelo serviço;
4. DPAPI depende da identidade `LocalService`, perfil carregado e segurança do host;
5. mídia de backup/recovery pode desaparecer ou ser adulterada;
6. instalação/repair/update são eventos privilegiados de supply chain.

## Ameaças e controles

| Ameaça                           | Controle                                                                        | Limite residual                                                        |
| -------------------------------- | ------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| furto/cópia do disco             | SQLCipher, DPAPI, ACL e backup autenticado                                      | processo desbloqueado e administrador hostil permanecem fora do modelo |
| acesso direto ao banco pela rede | somente API; banco/WAL em disco local; rejeição de UNC/SMB                      | indisponibilidade do host interrompe a clínica                         |
| interceptação/MITM na LAN        | CA por instalação, TLS, fingerprint e pareamento one-shot                       | cliente precisa confiar corretamente na CA                             |
| roubo/fixação de sessão          | token 256-bit, somente hash no banco, cookie `__Host-`, rotação e revogação     | malware no navegador/host pode agir como usuário autenticado           |
| CSRF/DNS rebinding               | SameSite Strict, token CSRF, `Origin` e `Host` estritos                         | configuração incorreta de hostname deve falhar fechada                 |
| brute force                      | Argon2id, resposta genérica, hash fictício e rate limit em memória              | DoS de rede ainda requer firewall/segmentação adequados                |
| XSS/frontend comprometido        | CSP, assets locais, cookies HttpOnly e API de menor privilégio                  | XSS autenticado ainda pode chamar capacidades permitidas pela sessão   |
| cache clínico no cliente         | `no-store`, SW limitado ao shell, sem IndexedDB/localStorage                    | cache do navegador/OS deve ser verificado por plataforma               |
| setup pela LAN                   | listener administrativo separado em loopback                                    | malware local no host continua relevante                               |
| arquivo de backup malicioso      | magic/versão/limites, HMAC, AEAD, SQLCipher e integridade                       | restore completo ainda não está implementado                           |
| path traversal/mídia trocada     | IDs de volume, canonicalização, destino fora dos dados ativos e escrita atômica | remoção física durante I/O deve ser exercitada                         |

## SQLCipher e chave do banco

- `K_db` tem 32 bytes de CSPRNG e não deriva da senha do administrador.
- SQLCipher recebe a chave antes de qualquer leitura e valida `cipher_status`, `cipher_integrity_check`, `integrity_check` e `foreign_key_check`.
- `foreign_keys=ON`, temporários em memória, extensões dinâmicas desabilitadas e nenhum fallback em claro.
- a versão integrada é consultada por `PRAGMA cipher_version`; distribuição exige **4.17.0 ou superior**.
- a dependência Rust que incorpora 4.17.0 é fixada por revisão Git e pelo `Cargo.lock`, porque a release publicada anterior ainda incorporava 4.14.0.
- `cipher_memory_security` só pode ser habilitado após teste Windows específico de quota/`VirtualLock`; a versão por si só não prova segurança operacional.
- chave, SQL contendo chave e conteúdo clínico não entram em trace, panic, API ou frontend.

### DPAPI do serviço

`K_db` e chaves privadas TLS são protegidas com `CryptProtectData`, `CRYPTPROTECT_UI_FORBIDDEN`, escopo `CurrentUser` e entropia contextual. `LOCAL_MACHINE` é proibido. O Windows Service usa `LocalService` com perfil carregado e HKCU próprio.

Isso vincula o envelope local à conta/máquina. O `.odskey` continua obrigatório para recuperação portátil. Antes do pacote portátil ser aceito, testes devem provar que o serviço consegue proteger e reabrir as chaves após reboot sem sessão interativa e que outro usuário não consegue fazê-lo.

## Senhas

A política segue o ADR 005:

- 15–128 code points Unicode após NFC, espaços permitidos;
- sem composição ou expiração periódica obrigatória;
- blocklist offline e termos contextuais;
- Argon2id v19, salt 16 bytes, saída 32 bytes, `m=65536 KiB`, `t=3`, `p=1`;
- hash fictício equivalente para username inexistente;
- nenhum hash/senha em log, auditoria ou resposta.

O parâmetro precisa de benchmark no hardware mínimo. Rate limit em memória é suficiente para um processo; reiniciar o serviço não pode tornar o endpoint uma forma prática de contornar controles de rede.

## Sessão e CSRF

- token opaco aleatório de 32 bytes; banco guarda somente SHA-256;
- cookie `__Host-dental_session; Secure; HttpOnly; SameSite=Strict; Path=/`, sem `Domain`;
- 30 minutos de inatividade e 12 horas absolutas;
- timestamps de última atividade são atualizados de forma controlada para não gerar escrita a cada asset;
- logout e rotação revogam o registro anterior antes de emitir novo token;
- CSRF aleatório é rotativo, enviado em header e persistido somente como hash;
- toda mutação valida sessão, CSRF, `Origin`, `Host` e autorização;
- operações críticas futuras pedem senha novamente e registram auditoria.

Tokens não entram em URL, HTML, JSON persistido, log, auditoria, `localStorage`, `sessionStorage` ou IndexedDB.

## TLS e pareamento

- CA por instalação: cinco anos;
- certificado leaf: 397 dias, renovação 30 dias antes;
- SAN estrito para `localhost` e `dental-<installation-id>.local`;
- algoritmo e biblioteca vêm de implementação mantida; não há primitiva própria;
- token de pareamento é CSPRNG, expira em dez minutos e só pode ser consumido uma vez;
- QR code contém somente material de pareamento não clínico e fingerprint verificável;
- uma restauração em outra máquina gera nova CA e revoga implicitamente a confiança anterior.

A janela de pareamento não autentica na aplicação e não expõe endpoints clínicos. Firewall abre 8743 somente em perfis Private/Domain.

## Cache e PWA

O Service Worker usa allowlist exata de assets com hash. Navegação pode receber o shell offline, mas qualquer `/api/`, resposta autenticada ou dado de negócio é network-only e `Cache-Control: no-store`. Não há Background Sync nem fila de mutações. Perda do servidor encerra a experiência operacional em modo somente informativo.

## Backup e recovery

- `.odskey` cifra somente `K_db` por XChaCha20-Poly1305 com header exato como AAD;
- `.odsbackup` contém snapshot SQLCipher e HMAC-SHA256 derivado por HKDF;
- pacote, backup e código permanecem separados;
- backup/recovery devem usar volumes locais, graváveis e distintos entre si;
- escrita usa `.partial`, sync, releitura, validação e rename atômico sem overwrite;
- segredos não aparecem em nomes, manifests, logs ou histórico.

Consulte [backup-and-restore.md](backup-and-restore.md).

## Logs e erros públicos

Logs técnicos registram código, resultado, timestamp e `correlationId`, nunca payload completo. Auditoria é append-only e não contém senha, PHC, cookie, CSRF, recovery code, `K_db`, chave TLS, conteúdo clínico integral ou path com PII.

O processo desabilita o log interno padrão do SQLCipher antes de aplicar a chave, mantendo causas criptográficas fora de `stderr` e dos logs operacionais.

Erros HTTP não revelam causa interna. Autenticação inválida sempre usa a mesma resposta pública. Panic não deve devolver backtrace ao cliente.

## Gates antes de distribuição ou dados reais

- [ ] SQLCipher runtime ≥ 4.17 e testes de cifra/corrupção aprovados no Windows.
- [ ] Serviço `LocalService`, perfil, ACL e reboot validados em Windows 11 limpo.
- [ ] TLS, renovação e pareamento exercitados em Edge/Chrome Windows e Android e Safari iOS.
- [ ] Cache Storage, IndexedDB e Service Worker provam ausência de `/api/` e dados clínicos.
- [ ] Teste de 20 clientes e reinício durante sessões/escritas não corrompe estado.
- [ ] Restore completo e rollback exercitados em outra máquina.
- [ ] Pacote portátil com binário assinado, firewall restrito e instalação/desinstalação preservando dados.
- [ ] Licença do produto, SBOM/atribuições e revisão LGPD definidas.
- [ ] Nenhum segredo em logs, respostas, bundle, dumps de teste ou snapshots.

## Referências

- [Microsoft — CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
- [Microsoft — Service User Accounts](https://learn.microsoft.com/en-us/windows/win32/services/service-user-accounts)
- [SQLCipher API](https://www.zetetic.net/sqlcipher/sqlcipher-api/)
- [NIST SP 800-63B-4 — Passwords](https://pages.nist.gov/800-63-4/sp800-63b.html#passwords)
