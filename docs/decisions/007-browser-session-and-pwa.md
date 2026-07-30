# ADR 007 — Sessão no navegador, CSRF e PWA limitada

- **Status:** aceita
- **Data:** 2026-07-22

## Contexto

A mudança para navegadores cria autenticação concorrente e uma origem HTTPS, mas a aplicação continua offline em relação à internet. Dados clínicos não podem sobreviver no storage do cliente nem ser editados quando o servidor estiver indisponível.

## Decisão

### Sessões

- token opaco aleatório de 256 bits; somente SHA-256 do token é persistido no SQLCipher;
- cookie `__Host-dental_session` com `Secure`, `HttpOnly`, `SameSite=Strict`, `Path=/` e sem `Domain`;
- expiração por inatividade de 30 minutos e absoluta de 12 horas;
- rotação após autenticação e quando necessário, logout e revogação explícitos;
- `SessionStore` é uma porta implementada no SQLCipher para permitir substituição futura sem acoplar casos de uso;
- login inicial é restrito ao `MASTER_ADMIN`, com verificação Argon2id equivalente para usuário inexistente e rate limit em memória;
- ações críticas exigirão reautenticação, não apenas sessão válida.

### CSRF e origem

- um segredo CSRF rotativo fica associado à sessão e apenas seu hash é persistido;
- requisições mutáveis exigem o valor no header definido pela API;
- `Origin` e `Host` são validados contra a origem efetivamente servida;
- respostas de autenticação e API usam `Cache-Control: no-store`;
- erros públicos têm apenas `code`, `message`, `correlationId` e `fieldErrors`.

### PWA

O Service Worker pode guardar somente arquivos estáticos versionados do shell. Rotas `/api/`, respostas autenticadas e dados clínicos ficam fora de Cache Storage, IndexedDB, `localStorage` e qualquer fila offline. Sem LAN, o shell mostra “servidor indisponível” e bloqueia edição.

## TLS e pareamento

- uma CA exclusiva por instalação vale cinco anos;
- o certificado do servidor vale no máximo 397 dias e renova 30 dias antes;
- SAN inclui `localhost` e o hostname mDNS da instalação;
- uma janela de pareamento de dez minutos usa token aleatório, one-shot e fingerprint SHA-256 verificável;
- a janela não expõe API autenticada nem dado clínico;
- as chaves privadas são protegidas pela conta do serviço.

O pareamento reduz erro operacional, mas não contorna controles do sistema operacional: cada plataforma ainda precisa instalar/confiar explicitamente na CA privada.

## Redis/Valkey

Não é dependência do MVP. Um processo com SQLCipher e rate limit em memória atende a até 20 clientes e preserva revogação após reinício. Redis/Valkey só será reconsiderado com múltiplas réplicas, coordenação distribuída ou gargalo medido; nunca armazenará o prontuário como fonte de verdade.

## Critérios de conformidade

- token em claro aparece somente no cookie da resposta e na memória transitória;
- cookie inválido, expirado ou revogado falha fechado;
- CSRF ausente/incorreto e `Origin`/`Host` não permitidos são rejeitados;
- logout revoga a sessão antes de expirar o cookie;
- pareamento expirado ou reutilizado é rejeitado;
- testes inspecionam Cache Storage/IndexedDB/Service Worker e provam ausência de respostas `/api/`.
