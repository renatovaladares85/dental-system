# ADR 006 — Servidor web nativo local

- **Status:** aceita; substitui os ADRs 001 e 004
- **Data:** 2026-07-22

## Contexto

Uma clínica deve operar com até 20 usuários na mesma LAN, inclusive em navegadores móveis, sem instalar Node, Rust, Docker ou um aplicativo próprio em cada dispositivo. O banco precisa continuar local, cifrado e acessado por um único processo. Não existe instalação Tauri com dados a migrar nesta estação.

## Decisão

Um único binário Rust executará como serviço do Windows e será a única autoridade sobre domínio, persistência, chaves e arquivos:

```text
navegador → HTTPS/API JSON → Axum → casos de uso → portas → SQLCipher/DPAPI/filesystem
```

- administração e configuração inicial escutam somente em `127.0.0.1:8742`;
- o listener HTTPS da LAN usa a porta `8743` e só é habilitado após `READY`;
- a SPA compilada pelo Vite é incorporada ao binário, sem CDN ou runtime Node;
- o serviço usa `LocalService`, perfil carregado, SID restrito e ACL exclusiva em `%ProgramData%\OfflineDentalSystem`;
- DPAPI permanece em escopo `CurrentUser`, agora no HKCU da conta de serviço, sem `LOCAL_MACHINE`;
- SQLCipher tem um único worker/ator, WAL no disco local e transações curtas;
- o arquivo do banco nunca é compartilhado por SMB e não é aberto pelos clientes;
- descoberta local anuncia `dental-<installation-id>.local` por mDNS;
- restauração em outro host cria nova CA e exige novo pareamento dos clientes.

O diretório histórico `src-tauri/` poderá permanecer como raiz do crate durante esta migração para reduzir churn; nenhuma dependência, configuração, command ou runtime Tauri pode permanecer no produto.

## Instalação e operação

O MSI instala o binário e o serviço, restringe a regra de firewall aos perfis Private/Domain, cria o atalho administrativo e aguarda o health check antes de abrir o navegador. Repair e uninstall preservam banco, chaves e backups por padrão.

O MSI não é distribuível enquanto licença do produto, certificado de assinatura, restauração completa e demais gates de segurança não estiverem concluídos. O build de desenvolvimento não implica aprovação de release.

## Consequências

Benefícios:

- um único host concentra atualização, criptografia, backup e autorização;
- clientes precisam apenas de navegador atual e pareamento inicial;
- o processo é mais leve que Docker e evita um runtime por estação;
- o SQLite continua em armazenamento local e atrás de uma única API.

Custos e riscos:

- a LAN vira uma fronteira de ataque e exige TLS, sessão, CSRF, validação de origem e rate limit;
- o host é ponto único de falha e deve ter energia, backup e operação controlados;
- confiança na CA privada exige procedimento por plataforma cliente;
- Safari/iOS, Android e políticas corporativas de certificados exigem testes reais;
- o serviço não deve depender de uma sessão interativa para abrir chaves.

## Alternativas rejeitadas

- **Tauri por dispositivo:** não atende ao acesso móvel e multiplica instalação/manutenção.
- **Docker no Windows host:** adiciona runtime, imagens, volumes e operação sem benefício para um único processo.
- **Arquivo SQLite em compartilhamento:** viola o modelo de acesso adotado e aumenta risco de locking/corrupção.
- **Servidor em nuvem ou acesso pela internet:** amplia escopo, dependências e exposição além do requisito da clínica.
- **Redis/Valkey:** não há réplica distribuída nem gargalo medido que justifique outro serviço.

## Critérios de conformidade

- setup/recovery retornam `404` ou `403` fora do listener loopback;
- listener LAN não inicia antes de `READY`;
- API, logs e frontend não contêm chave, senha, token ou CSRF em claro;
- o serviço reinicia com o Windows e mantém uma única instância;
- nenhuma operação abre o banco por caminho de rede;
- a máquina cliente não precisa de Node, Rust, Docker ou WebView2 empacotado pelo produto.

## Referências

- [SQLite — Appropriate Uses](https://www.sqlite.org/whentouse.html)
- [Microsoft — Service User Accounts](https://learn.microsoft.com/en-us/windows/win32/services/service-user-accounts)
- [Microsoft — LocalService Account](https://learn.microsoft.com/en-us/windows/win32/services/localservice-account)
