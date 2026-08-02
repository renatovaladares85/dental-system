# Offline Dental System

Servidor web local para uma clínica odontológica. Um único processo Rust no Windows 11 mantém os dados em SQLCipher e entrega a interface React por HTTPS para navegadores da mesma LAN. Clientes não precisam de Node, Rust, Docker, Redis ou aplicativo próprio.

> **Estado:** fundação técnica. Setup inicial, login do `MASTER_ADMIN`, sessões, pareamento e backup inicial estão no escopo; módulos clínicos, restauração completa e distribuição assinada ainda não estão liberados para dados reais.

## Instalação para usuário final

O único instalador suportado é o MSI assinado. A release oficial disponibilizará
`OfflineDentalSystem-<versão>-windows-x64.msi`, checksum, SBOM e instruções de
instalação. Um ZIP de release, quando publicado, será apenas um contêiner desses
arquivos: ele não cria serviço, altera ACL, firewall ou certificados.

Não execute scripts de pacote portátil/ZIP como instalação de produção. A
desinstalação padrão deve preservar `%ProgramData%\OfflineDentalSystem`, incluindo
banco, chaves e backups.

## Iniciar no Windows para desenvolvimento

Na raiz do repositório, execute no PowerShell 7 não elevado:

```powershell
pwsh -NoProfile -File .\scripts\start-windows.ps1
```

O orquestrador executa `check`, `build` e `run`. Ferramentas ausentes nunca são
instaladas implicitamente. O host usa somente `.local-data\dev-host\Data`, aguarda
o health e só abre o navegador com `-OpenBrowser`.

## Topologia

| Endereço                                      | Exposição                 | Uso                                             |
| --------------------------------------------- | ------------------------- | ----------------------------------------------- |
| `http://127.0.0.1:8742`                       | somente host              | setup, volumes, health e abertura do pareamento |
| `https://dental-<installation-id>.local:8743` | LAN, somente após `READY` | SPA, login, sessão e APIs da aplicação          |

- banco, regras, autenticação, criptografia e paths ficam em Rust;
- React usa `fetch` same-origin e nunca recebe acesso genérico a SQL ou filesystem;
- sessões persistem no SQLCipher; somente hashes de token e CSRF são gravados;
- a PWA guarda apenas o shell estático e não intercepta `/api/`;
- cada instalação possui CA e certificado próprios, com pareamento one-shot por QR code;
- o arquivo SQLite nunca é compartilhado por SMB/UNC.

## Comandos de qualidade

```powershell
npm ci
npm run format:check
npm run lint
npm run typecheck
npm test
npm run build

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked --all-features
cargo build --manifest-path src-tauri/Cargo.toml --locked --all-features
```

O nome histórico `src-tauri/` permanece apenas como raiz do crate para evitar churn. O produto não usa runtime, IPC, WebView ou dependências Tauri.

## Documentação

- [Arquitetura](docs/architecture.md)
- [Desenvolvimento e operação Windows](docs/development.md)
- [Logs operacionais e auditoria](docs/observability-and-audit.md)
- [Modelo de segurança](docs/security.md)
- [Backup e recuperação](docs/backup-and-restore.md)
- [Modelo de dados](docs/data-model.md)
- [Decisões arquiteturais](docs/decisions/README.md)

## Limites de distribuição

O código fixa uma revisão do `rusqlite` que incorpora SQLCipher 4.17.0 e também valida a versão em runtime. Isso não libera produção sozinho. Serviço/ACL/DPAPI após reboot, TLS e renovação, cache dos navegadores, 20 clientes concorrentes, restore em outra máquina e instalação pelo MSI em Windows 11 limpo ainda precisam passar pelos gates documentados em [security.md](docs/security.md).

Não há publicação, release, updater ou telemetria nesta etapa.
