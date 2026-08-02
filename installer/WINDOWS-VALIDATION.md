# Validação do MSI no Windows

Execute estes gates em uma VM Windows 11 x64 limpa. O MSI é o único instalador
suportado; scripts ZIP e bootstrapper não fazem parte da validação.

## Build

- `npm ci`, qualidade frontend e build da SPA concluídos;
- build Rust release concluído com assets reais incorporados;
- `--security-diagnostics --json` informa SQLCipher `>= 4.17.0` e
  `distributionReady: true`;
- WiX `4.0.6` e as extensões `WixToolset.Firewall.wixext 4.0.6` e
  `WixToolset.Util.wixext 4.0.6` estão provisionados localmente;
- o bootstrap é executado sem elevação e a primeira execução requer acesso ao
  NuGet oficial:

  ```powershell
  $wixTools = .\scripts\tools\prepare-wix.ps1
  ```

- validações posteriores reutilizam o CLI e as DLLs locais, sem cache global e
  sem rede durante o build:

  ```powershell
  .\installer\build-msi.ps1 `
      -ValidationOnly `
      -WixExecutable $wixTools.WixExecutable `
      -WixExtensionRoot $wixTools.ExtensionRoot
  ```

- `ValidationOnly` conclui sem `.msi`, `.partial`, diretório de distribuição ou
  mudança no working tree;
- em distribuição, o executável e o MSI possuem Authenticode, certificado
  esperado e timestamp válido.

## Tooling WiX isolado

O bootstrap não instala ferramentas globalmente. Os arquivos ficam sob:

```text
.local-data\tools\wix\4.0.6\
.local-data\tools\wix-extensions\<pacote>\4.0.6\
.local-data\downloads\
.local-data\staging\
.local-data\backups\tools\
.local-data\logs\wix\
```

Diretórios incompletos são movidos para `.local-data\backups\tools` com nome
único; não os apague antes de concluir o diagnóstico. Falhas WiX preservam
stdout e stderr em `.local-data\logs\wix`, junto com etapa, comando e código de
saída exibidos no console. Logs de execuções bem-sucedidas são removidos.

Para reprovisionar manualmente, pare o host e mova `.local-data` integralmente
para um local de backup fora do repositório antes de repetir o bootstrap. Não
remova arquivos versionados nem use limpeza ampla do Git. `ValidationOnly` não
gera um MSI distribuível.

## Serviço e dados

- `OfflineDentalSystem` usa `NT AUTHORITY\LocalService`, delayed auto-start e
  Service SID restrito;
- o SCM permanece `StartPending` até o listener administrativo estar ligado e
  só então publica `Running`;
- `%ProgramData%\OfflineDentalSystem` preserva banco, DPAPI, backups e
  identidade em repair, upgrade, rollback e uninstall;
- regras de firewall permanecem somente em `Private`/`Domain`; não há exceção
  em rede pública;
- falha anterior à criação do serviço não tenta `stop` ou `delete`.

## Operação

- a instalação não abre navegador como condição de sucesso;
- `GET http://127.0.0.1:8742/api/v1/health` devolve HTTP 200,
  JSON `{"status":"ok"}`, `Content-Type: application/json` e
  `Cache-Control: no-store`;
- logs sanitizados ficam em `%ProgramData%\OfflineDentalSystem\logs\runtime`;
- use `scripts/diagnostics/inspect-installation.ps1` para inspeção somente
  leitura antes de qualquer ação corretiva.
