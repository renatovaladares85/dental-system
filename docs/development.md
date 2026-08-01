# Desenvolvimento e operação no Windows

## Ambientes

| Ambiente                                  | Desenvolvimento          | Produção futura  |
| ----------------------------------------- | ------------------------ | ---------------- |
| Windows 11 x64 atualizado                 | suportado e preferencial | alvo principal   |
| Windows 10 com edição/ESU ainda suportado | validação adicional      | suporte restrito |
| Windows x86/ARM, macOS ou Linux           | edição e testes parciais | fora do escopo   |

Builds, DPAPI, serviço, ACL, firewall, mDNS e instalação por script devem ser validados nativamente em Windows/MSVC. WSL é útil para edição e verificações independentes da plataforma, mas não substitui essa validação.

## Caminho recomendado

Na raiz do repositório, execute no PowerShell 7 não elevado:

```powershell
pwsh -NoProfile -File .\scripts\start-windows.ps1
```

O orquestrador executa, em ordem:

1. valida Windows 11 x64;
2. recusa execução elevada, UNC, SMB, unidade mapeada e reparse point;
3. recusa portas 8742 e 8743 ocupadas por outra instância;
4. valida Node 24.17.0/npm 11 e Rust 1.97.1 MSVC;
5. valida Build Tools C++, Windows SDK, Perl e NASM;
6. executa `npm ci` e toda a qualidade frontend;
7. executa `cargo fmt`, Clippy, testes e build com `--locked`;
8. inicia uma instância de desenvolvimento isolada em `.local-data\dev-host\Data`;
9. valida corpo e headers de `GET /api/v1/health`; o navegador só abre com `-OpenBrowser`.

Ferramentas ausentes falham com instrução objetiva; a instalação delas é um bootstrap
manual separado. O iniciador de desenvolvimento não instala serviço, não altera
firewall e não confia em CA.

## Pré-requisitos manuais

- Git para Windows;
- Node.js 24.17.0 LTS, conforme `.nvmrc`, com npm 11;
- Rust 1.97.1, `rustfmt`, Clippy e target `x86_64-pc-windows-msvc`;
- Visual Studio 2022 Build Tools com Desktop development with C++ e Windows SDK;
- Perl e NASM para OpenSSL/SQLCipher vendorizados;
- PowerShell 7 obrigatório.

O usuário final não precisa desses componentes: o pacote portátil contém o binário Rust e a SPA incorporada.

## Execução manual

Frontend com proxy para o listener administrativo:

```powershell
npm ci
npm run dev
```

Build integrado do servidor:

```powershell
npm run build
cargo build --manifest-path src-tauri/Cargo.toml --locked --all-features
```

O `build.rs` incorpora `dist/` ao executável. Build release sem SPA válida falha; em debug/teste, o fallback existe apenas para permitir testes Rust isolados.

Qualidade equivalente à CI:

```powershell
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

## Dados de desenvolvimento e produção

O script usa somente `.local-data/dev-host/Data` no repositório, ignorado pelo Git. A instalação MSI usa:

```text
%ProgramData%\OfflineDentalSystem\
  host-identity.json
  Data\
    active\
      database.sqlcipher
      database-key.dpapi.json
  Artifacts\
  tls\
  logs\
```

Em outro volume, o destino controlado é `X:\OfflineDentalSystem\Artifacts`. O navegador envia apenas IDs opacos retornados pelo servidor. UNC/SMB, volumes de rede, somente leitura, reparse points inesperados, dois IDs iguais e destinos dentro de `Data\active` falham fechados.

Não copie banco/WAL ao vivo, não edite envelopes DPAPI e não envie banco, `.odskey`, `.odsbackup`, logs ou códigos de recuperação a issues.

## Migrations

Migrations em `src-tauri/migrations/` são forward-only e imutáveis depois de compartilhadas:

- `0001_foundation.sql`: instalação, organização, unidade, master, configuração, backup/recovery e auditoria;
- `0002_web_identity_sessions.sql`: sessões web, expiração, revogação e índices.
- `0003_operational_audit.sql`: resultado, correlação, sessão, origem e índices da auditoria.

Teste sempre banco vazio e upgrade da versão anterior. Não execute SQL manual em dados reais. A auditoria possui triggers contra `UPDATE` e `DELETE`.

## SQLCipher

O Cargo fixa a revisão `4707a1fce4d1bdbc2c4fc7b35266c13e31643cd8` do `rusqlite`, cujo amalgamation incorporado declara SQLCipher 4.17.0. O `Cargo.lock` fixa o grafo, mas o gate autoritativo continua sendo a versão real consultada por `PRAGMA cipher_version` no executável Windows.

O servidor também executa verificações de cifra, integridade e foreign keys. Nunca coloque `PRAGMA key`, senha, cookie, CSRF ou código de recuperação em console, log ou captura.

O log interno do SQLCipher é desabilitado antes da chave (`cipher_log_level = NONE`); diagnóstico operacional usa apenas códigos sanitizados e a versão pública da biblioteca.

`cipher_memory_security` permanece desabilitado até o teste Windows específico de quota/`VirtualLock` ser aprovado. Isso é um gate operacional, não um motivo para enfraquecer a cifra em disco.

## Serviço e instalação por MSI

O alvo operacional é um Windows Service sob `LocalService`, início automático, perfil carregado e SID restrito. O instalador deve:

- conceder ACL somente ao SID do serviço, `SYSTEM` e administradores;
- abrir TCP 8743 somente nos perfis Private/Domain;
- iniciar o serviço e aguardar readiness antes de publicar `Running`;
- disponibilizar atalho para `http://127.0.0.1:8742` sem abri-lo como condição de sucesso;
- preservar `%ProgramData%\OfflineDentalSystem` em repair/uninstall.

Empacotamento utilizável deve falhar enquanto licença do produto, certificado de assinatura, SQLCipher runtime ≥ 4.17 ou gates de segurança estiverem ausentes. CI não publica, não assina e não faz upload de artefatos.

O MSI é o único mecanismo suportado para criar/configurar o serviço. `-ValidationOnly`
compila e valida o MSI com fixture temporário, sem assinar nem publicar. A distribuição
exige executável e MSI assinados, com timestamp válido, antes de produzir assets de
release. Scripts ZIP legados não são caminho suportado.

## Diagnóstico seguro

| Sintoma                       | Ação                                                             |
| ----------------------------- | ---------------------------------------------------------------- |
| health loopback indisponível  | verificar processo/serviço e log sanitizado; não recriar dados   |
| `link.exe`/SDK ausente        | reparar workload C++ e carregar Developer PowerShell             |
| OpenSSL/SQLCipher não compila | confirmar MSVC x64, Perl e NASM                                  |
| navegador não confia no host  | repetir pareamento e comparar SHA-256 antes de importar a CA     |
| hostname `.local` não resolve | testar mesma LAN, perfil de firewall e mDNS                      |
| banco retorna `NOTADB`        | parar, preservar cópia e conferir key/database ID                |
| DPAPI falha após reboot       | confirmar conta `LocalService`/perfil; não sobrescrever envelope |
| setup interrompido            | retomar pelo host e revalidar ambos os volumes                   |

Quando a causa não estiver clara, interrompa operações de escrita e trabalhe em cópias verificáveis. Consulte o [modelo de segurança](security.md) antes de qualquer uso real.
