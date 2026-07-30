# Checklist do instalador Windows

Este checklist é obrigatório antes de considerar um MSI utilizável. Execute em
uma máquina virtual Windows 11 x64 limpa, nos perfis de rede `Private` e
`Domain`, sem Node.js, Rust, Docker ou sessão de desenvolvimento.

## Gate antes da instalação

- O executável e o MSI possuem assinatura Authenticode válida e timestamp.
- `--security-diagnostics --json` informa SQLCipher `>= 4.17.0` e
  `distributionReady: true`.
- A licença de produto definitiva foi incorporada; o marcador
  `validation-NOT-FOR-DISTRIBUTION.txt` não está presente.

## Serviço, identidade e dados

- O serviço `OfflineDentalSystem` executa como `NT AUTHORITY\LocalService`,
  inicia automaticamente com atraso e possui SID do tipo `RESTRICTED`.
- O SID retornado por `sc.exe showsid OfflineDentalSystem` é
  `S-1-5-80-3281840523-3983707945-848950836-1812796060-3499222651`.
- `%ProgramData%\OfflineDentalSystem` possui DACL protegida somente para
  `SYSTEM`, `BUILTIN\Administrators` e o SID do serviço; usuários comuns não
  conseguem ler banco, envelopes DPAPI ou chaves TLS.
- A CA instalada em `LocalMachine\Root` corresponde byte a byte a
  `%ProgramData%\OfflineDentalSystem\tls\ca.cer` e não contém chave privada.
- Repair e upgrade preservam banco, identidade, chaves e artefatos. Uninstall
  remove serviço/binários/regras, mas preserva os dados por padrão.

## Rede e descoberta

- Antes de `READY`, somente `127.0.0.1:8742` responde; `8743` e o anúncio mDNS
  permanecem fechados.
- Depois de `READY`, TCP `8743` e UDP `5353` possuem regras inbound separadas,
  limitadas a `Private`/`Domain` e `LocalSubnet`; o perfil `Public` não possui
  exceção.
- Um cliente da mesma LAN resolve `dental-<installation-id>.local`, alcança
  HTTPS e valida o fingerprint exibido no pareamento.
- Todo endereço A/AAAA anunciado por mDNS realmente aceita HTTPS em `8743`;
  não pode haver anúncio IPv6 com listener somente IPv4.

## Operação e clientes

- O atalho e a abertura pós-health-check funcionam sem terminal e sem
  dependências de desenvolvimento.
- Edge e Chrome no Windows/Android e Safari no iOS concluem confiança da CA,
  pareamento, login, logout e expiração/revogação de sessão.
- O firewall volta a bloquear o tráfego ao trocar a rede para `Public`.
- O Service Worker não armazena `/api`, dados clínicos, cookies ou CSRF em
  Cache Storage/IndexedDB e apresenta apenas o shell quando o servidor cai.

