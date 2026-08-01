# Checklist da instalação local no Windows

Este checklist é obrigatório antes de considerar o pacote portátil utilizável. Execute em
uma máquina virtual Windows 11 x64 limpa, nos perfis de rede `Private` e
`Domain`, sem Node.js, Rust, Docker ou sessão de desenvolvimento.

## Gate antes da instalação

- O executável possui assinatura Authenticode válida e timestamp; o bootstrapper fixa o mesmo publisher.
- `--security-diagnostics --json` informa SQLCipher `>= 4.17.0` e
  `distributionReady: true`.
- A licença de produto definitiva foi incorporada; o marcador
  o fixture temporário de validação é criado fora do repositório e removido ao fim.

## Serviço, identidade e dados

- O serviço `OfflineDentalSystem` executa como `NT AUTHORITY\LocalService`,
  inicia automaticamente com atraso e possui SID do tipo `RESTRICTED`.
- `Get-CimInstance Win32_Service -Filter "Name='OfflineDentalSystem'"` apresenta
  `PathName` exatamente como `"<executável controlado>" --service` e
  `StartName` como `NT AUTHORITY\LocalService`. `sc.exe qc` e
  `sc.exe qsidtype` devem confirmar os mesmos valores.
- Em `HKLM\SYSTEM\CurrentControlSet\Services\OfflineDentalSystem`, `Start = 2`,
  `DelayedAutoStart = 1` e `ServiceSidType = 3`.
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

- Com o sistema ausente, `Instalar-e-Iniciar.bat` valida o ZIP, solicita UAC uma vez, instala, espera o health e abre o navegador.
- Com o sistema saudável, uma nova execução do BAT ou do atalho abre o navegador sem reinstalar.
- Com o serviço parado, `Abrir-Sistema.bat` solicita UAC, inicia o serviço e abre o navegador.
- ZIP ausente, duplicado, adulterado, truncado ou com binário assinado por outro certificado é rejeitado com mensagem legível.
- Cancelar o UAC não inicia instalação parcial nem remove dados existentes.
- Uma falha em `sc.exe create` não executa `stop`/`delete`; uma reexecução
  atualiza apenas um serviço cujo executável anterior pertença ao diretório
  controlado do produto. Serviço homônimo conflitante é recusado sem alteração.
- Os atalhos “Offline Dental System” existem na Área de Trabalho e no menu Iniciar, usam o ícone local e abrem somente `http://127.0.0.1:8742`.
- A desinstalação padrão preserva `%ProgramData%\OfflineDentalSystem`; a remoção total exige digitar `REMOVER`.
- O atalho e a abertura pós-health-check funcionam sem terminal e sem
  dependências de desenvolvimento.
- Edge e Chrome no Windows/Android e Safari no iOS concluem confiança da CA,
  pareamento, login, logout e expiração/revogação de sessão.
- O firewall volta a bloquear o tráfego ao trocar a rede para `Public`.
- O Service Worker não armazena `/api`, dados clínicos, cookies ou CSRF em
  Cache Storage/IndexedDB e apresenta apenas o shell quando o servidor cai.
