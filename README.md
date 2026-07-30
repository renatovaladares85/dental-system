# Offline Dental System

Servidor web local para uma clínica odontológica. Um único processo Rust no Windows 11 mantém os dados em SQLCipher e entrega a interface React por HTTPS para navegadores da mesma LAN. Clientes não precisam de Node, Rust, Docker, Redis ou aplicativo próprio.

> **Estado:** fundação técnica. Setup inicial, login do `MASTER_ADMIN`, sessões, pareamento e backup inicial estão no escopo; módulos clínicos, restauração completa e distribuição assinada ainda não estão liberados para dados reais.

## Iniciar no Windows para desenvolvimento

Na raiz do repositório, execute no PowerShell ou no Prompt de Comando:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start-windows.ps1
```

O script valida as versões fixadas, instala dependências do lockfile, executa formatação, lint, typecheck, testes e builds, inicia o servidor, aguarda o health check e abre `http://127.0.0.1:8742` no navegador.

Por segurança, ferramentas ausentes **não são instaladas implicitamente**. Em uma estação de desenvolvimento autorizada, a instalação assistida por `winget` é opt-in:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start-windows.ps1 -InstallMissing
```

Esse script é para quem desenvolve a partir do código-fonte. O usuário final usará o MSI assinado e não executará comandos nem instalará toolchains. A geração de um MSI utilizável permanece bloqueada até aprovação da licença do produto, certificado de assinatura e gates de segurança.

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

.\scripts\windows-cargo.cmd fmt --manifest-path src-tauri/Cargo.toml --all -- --check
.\scripts\windows-cargo.cmd clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings
.\scripts\windows-cargo.cmd test --manifest-path src-tauri/Cargo.toml --locked --all-features
.\scripts\windows-cargo.cmd build --manifest-path src-tauri/Cargo.toml --locked --all-features
```

O nome histórico `src-tauri/` permanece apenas como raiz do crate para evitar churn. O produto não usa runtime, IPC, WebView ou dependências Tauri.

## Documentação

- [Arquitetura](docs/architecture.md)
- [Desenvolvimento e operação Windows](docs/development.md)
- [Modelo de segurança](docs/security.md)
- [Backup e recuperação](docs/backup-and-restore.md)
- [Modelo de dados](docs/data-model.md)
- [Decisões arquiteturais](docs/decisions/README.md)

## Limites de distribuição

O código fixa uma revisão do `rusqlite` que incorpora SQLCipher 4.17.0 e também valida a versão em runtime. Isso não libera produção sozinho. Serviço/ACL/DPAPI após reboot, TLS e renovação, cache dos navegadores, 20 clientes concorrentes, restore em outra máquina e MSI em Windows 11 limpo ainda precisam passar pelos gates documentados em [security.md](docs/security.md).

Não há publicação, release, updater ou telemetria nesta etapa.
