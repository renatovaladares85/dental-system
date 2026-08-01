# Offline Dental System

Servidor web local para uma clínica odontológica. Um único processo Rust no Windows 11 mantém os dados em SQLCipher e entrega a interface React por HTTPS para navegadores da mesma LAN. Clientes não precisam de Node, Rust, Docker, Redis ou aplicativo próprio.

> **Estado:** fundação técnica. Setup inicial, login do `MASTER_ADMIN`, sessões, pareamento e backup inicial estão no escopo; módulos clínicos, restauração completa e distribuição assinada ainda não estão liberados para dados reais.

## Instalação para usuário final

O sistema é web local. O usuário não instala Node, Rust, Docker ou banco separadamente: recebe uma pasta com o bootstrapper e um ZIP pré-compilado, então executa somente:

```text
Instalar-e-Iniciar.bat
```

O script valida o pacote, solicita UAC, instala o servidor web como serviço local, configura ACL/firewall/certificado, cria atalhos, aguarda o health check e abre o navegador. Reexecuções saudáveis apenas abrem o sistema. O ZIP pode estar ao lado do BAT ou ser baixado por HTTPS mediante `canal-instalacao.json` com SHA-256 fixado.

Depois da instalação, o atalho “Offline Dental System” na Área de Trabalho inicia o serviço quando necessário e abre `http://127.0.0.1:8742`. O menu Iniciar também contém o desinstalador.

- `Desinstalar-Sistema.bat` remove serviço, firewall, certificado, programa e atalhos, preservando `%ProgramData%\OfflineDentalSystem`;
- `Desinstalar-Tudo.bat` também apaga banco, chaves e configurações somente após digitar `REMOVER`.

O pacote portátil é gerado por:

```powershell
.\scripts\build-portable-package.ps1
```

Distribuição real permanece bloqueada sem licença definitiva, binário Authenticode assinado, timestamp e gates de segurança. Para validação dentro do repositório é permitido gerar um pacote explicitamente marcado como desenvolvimento com `-Development`; ele nunca deve ser entregue a terceiros.

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

Esse script é exclusivo para desenvolvimento a partir do código-fonte. O usuário final recebe o pacote pré-compilado e não instala toolchains.

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
- [Logs operacionais e auditoria](docs/observability-and-audit.md)
- [Modelo de segurança](docs/security.md)
- [Backup e recuperação](docs/backup-and-restore.md)
- [Modelo de dados](docs/data-model.md)
- [Decisões arquiteturais](docs/decisions/README.md)

## Limites de distribuição

O código fixa uma revisão do `rusqlite` que incorpora SQLCipher 4.17.0 e também valida a versão em runtime. Isso não libera produção sozinho. Serviço/ACL/DPAPI após reboot, TLS e renovação, cache dos navegadores, 20 clientes concorrentes, restore em outra máquina e instalação pelo pacote portátil em Windows 11 limpo ainda precisam passar pelos gates documentados em [security.md](docs/security.md).

Não há publicação, release, updater ou telemetria nesta etapa.
