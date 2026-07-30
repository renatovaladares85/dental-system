# Desenvolvimento e operação no Windows

## Ambientes

| Ambiente                                  | Desenvolvimento          | Produção futura  |
| ----------------------------------------- | ------------------------ | ---------------- |
| Windows 11 x64 atualizado                 | suportado e preferencial | alvo principal   |
| Windows 10 com edição/ESU ainda suportado | validação adicional      | suporte restrito |
| Windows x86/ARM, macOS ou Linux           | edição e testes parciais | fora do escopo   |

Builds, DPAPI, serviço, ACL, firewall, mDNS e MSI devem ser validados nativamente em Windows/MSVC. WSL é útil para edição e verificações independentes da plataforma, mas não substitui essa validação.

## Caminho recomendado

Na raiz do repositório, execute no PowerShell ou no Prompt de Comando:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start-windows.ps1
```

O script é idempotente e executa, em ordem:

1. valida Windows 11 x64;
2. recusa execução elevada, UNC, SMB, unidade mapeada e reparse point;
3. reutiliza somente o processo de desenvolvimento exato; outra ocupação da porta 8742 falha fechada;
4. valida Node 24.17.0/npm 11 e Rust 1.97.1 MSVC;
5. valida Build Tools C++, Windows SDK, Perl e NASM;
6. executa `npm ci` e toda a qualidade frontend;
7. executa `cargo fmt`, Clippy, testes e build com `--locked`;
8. inicia uma instância de desenvolvimento isolada em `.local-data/`;
9. valida corpo e headers de `GET /api/v1/health` e abre o setup no navegador.

Por padrão ele apenas valida: não modifica a estação. A instalação assistida é permitida somente com opção explícita e requer internet/`winget`:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start-windows.ps1 -InstallMissing
```

Não eleve o PowerShell e não use `-InstallMissing` em servidor com dados reais. O `winget` solicita elevação separadamente quando um instalador precisa dela; builds e scripts de dependências continuam no token normal. O iniciador não instala serviço, não altera firewall, não confia em CA e não gera MSI.

O iniciador aguarda explicitamente cada processo e funciona no Windows PowerShell 5.1 ou PowerShell 7, inclusive em terminais com saída redirecionada. Quando a cópia portátil versionada de Strawberry Perl/NASM já existe no perfil, ela é reutilizada sem instalação global.

## Pré-requisitos manuais

- Git para Windows;
- Node.js 24.17.0 LTS, conforme `.nvmrc`, com npm 11;
- Rust 1.97.1, `rustfmt`, Clippy e target `x86_64-pc-windows-msvc`;
- Visual Studio 2022 Build Tools com Desktop development with C++ e Windows SDK;
- Perl e NASM para OpenSSL/SQLCipher vendorizados;
- PowerShell 7 recomendado.

O usuário final não precisa desses componentes: o MSI futuro conterá o binário Rust e a SPA incorporada.

## Execução manual

Frontend com proxy para o listener administrativo:

```powershell
npm ci
npm run dev
```

Build integrado do servidor:

```powershell
npm run build
.\scripts\windows-cargo.cmd build --manifest-path src-tauri/Cargo.toml --locked --all-features
```

O `build.rs` incorpora `dist/` ao executável. Build release sem SPA válida falha; em debug/teste, o fallback existe apenas para permitir testes Rust isolados.

Qualidade equivalente à CI:

```powershell
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

## Dados de desenvolvimento e produção

O script usa somente `.local-data/web-host/` no repositório, ignorado pelo Git. A instalação futura usa:

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

Teste sempre banco vazio e upgrade da versão anterior. Não execute SQL manual em dados reais. A auditoria possui triggers contra `UPDATE` e `DELETE`.

## SQLCipher

O Cargo fixa a revisão `4707a1fce4d1bdbc2c4fc7b35266c13e31643cd8` do `rusqlite`, cujo amalgamation incorporado declara SQLCipher 4.17.0. O `Cargo.lock` fixa o grafo, mas o gate autoritativo continua sendo a versão real consultada por `PRAGMA cipher_version` no executável Windows.

O servidor também executa verificações de cifra, integridade e foreign keys. Nunca coloque `PRAGMA key`, senha, cookie, CSRF ou código de recuperação em console, log ou captura.

O log interno do SQLCipher é desabilitado antes da chave (`cipher_log_level = NONE`); diagnóstico operacional usa apenas códigos sanitizados e a versão pública da biblioteca.

`cipher_memory_security` permanece desabilitado até o teste Windows específico de quota/`VirtualLock` ser aprovado. Isso é um gate operacional, não um motivo para enfraquecer a cifra em disco.

## Serviço e MSI

O alvo operacional é um Windows Service sob `LocalService`, início automático, perfil carregado e SID restrito. O instalador deve:

- conceder ACL somente ao SID do serviço, `SYSTEM` e administradores;
- abrir TCP 8743 somente nos perfis Private/Domain;
- iniciar o serviço, aguardar o health loopback e confiar na CA pública no host;
- criar atalho para `http://127.0.0.1:8742`;
- preservar `%ProgramData%\OfflineDentalSystem` em repair/uninstall.

Empacotamento utilizável deve falhar enquanto licença do produto, certificado de assinatura, SQLCipher runtime ≥ 4.17 ou gates de segurança estiverem ausentes. CI não publica, não assina e não faz upload de artefatos.

O authoring WiX pode ser compilado e validado sem produzir artefato distribuível:

```powershell
.\installer\build-msi.ps1 -ValidationOnly
```

Esse modo exige WiX 4.0.6, usa uma declaração marcada como não-licença, cria o MSI somente em diretório temporário e o remove ao terminar. O modo real exige `-ProductLicenseFile`, assinatura válida do executável e as variáveis seguras de assinatura/timestamp; recusa sobrescrever um MSI existente.

O WiX pode estar no `PATH` ou ser uma cópia portátil fixada em `ODS_WIX_EXE`. As extensões Firewall/Util 4.0.6 são adicionadas de forma idempotente ao cache da ferramenta. O gate real exige `distributionReady` booleano, diagnóstico em até 15 segundos e assinatura/timestamp do executável pelo thumbprint configurado. Nenhum desses componentes integra o runtime do produto.

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
