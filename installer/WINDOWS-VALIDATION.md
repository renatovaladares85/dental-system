# Validação do MSI no Windows

Execute estes gates em uma VM Windows 11 x64 limpa. O MSI é o único instalador
suportado; scripts ZIP e bootstrapper não fazem parte da validação.

## Build

- `npm ci`, qualidade frontend e build da SPA concluídos;
- build Rust release concluído com assets reais incorporados;
- `--security-diagnostics --json` informa SQLCipher `>= 4.17.0` e
  `distributionReady: true`;
- `installer/build-msi.ps1 -ValidationOnly` conclui sem arquivo ignorado;
- em distribuição, o executável e o MSI possuem Authenticode, certificado
  esperado e timestamp válido.

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
