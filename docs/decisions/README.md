# Decisões arquiteturais

| ADR                                            | Decisão                                       | Status                               |
| ---------------------------------------------- | --------------------------------------------- | ------------------------------------ |
| [001](001-tauri-rust-boundary.md)              | fronteira Tauri/Rust                          | substituída pelo ADR 006             |
| [002](002-sqlcipher-dpapi.md)                  | SQLCipher e proteção de chave por DPAPI       | aceita com gate de produção pendente |
| [003](003-recovery-envelope-initial-backup.md) | envelope de recovery e backup inicial         | aceita                               |
| [004](004-offline-windows-distribution.md)     | distribuição offline para Windows             | substituída pelo ADR 006             |
| [005](005-nist-password-policy.md)             | política de senha NIST e divergência do Figma | aceita                               |
| [006](006-native-web-service.md)               | serviço Rust, API local e operação Windows    | aceita                               |
| [007](007-browser-session-and-pwa.md)          | sessão, CSRF, TLS e cache do shell            | aceita                               |

ADRs registram decisões estáveis. Uma mudança incompatível cria novo ADR que referencia e substitui o anterior; não se reescreve a motivação histórica.
