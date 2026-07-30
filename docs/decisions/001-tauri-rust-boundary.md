# ADR 001 — Fronteira Tauri/Rust

- **Status:** substituída pelo [ADR 006](006-native-web-service.md)
- **Data:** 2026-07-22

## Contexto

O aplicativo processará dados pessoais e clínicos no mesmo computador em que executa um WebView. Expor SQL, filesystem ou chaves diretamente ao frontend reduziria a capacidade de aplicar autorização, transações, limites e auditoria em um ponto confiável.

## Decisão

Rust é a fronteira privilegiada e autoritativa. React/TypeScript implementa apresentação, acessibilidade, navegação e validação de experiência, mas acessa capacidades nativas somente por comandos Tauri específicos e tipados.

Fluxo obrigatório:

```text
componente → adapter de command → command Rust → caso de uso → porta/repository
```

- Commands são finos: desserializam/validam limites, obtêm contexto, chamam um caso de uso e mapeiam erro público.
- Casos de uso aplicam autorização e coordenam transações/auditoria.
- Regras vivem em domínio/políticas, não no command ou componente.
- Infraestrutura implementa SQLite/SQLCipher, DPAPI e filesystem.
- Não haverá `tauri-plugin-sql`, comando de SQL genérico ou filesystem irrestrito.
- Segredos e conteúdo integral desnecessário não atravessam IPC.
- Capabilities, CSP e command allowlist seguem deny-by-default.
- Operações SQLite bloqueantes usam worker/estado Rust, sem bloquear UI.
- Plugin de instância única impede duas instâncias gravando o mesmo banco.

## Consequências

Positivas:

- autorização e auditoria não podem ser ignoradas por navegação/UI;
- SQL e paths ficam encapsulados e testáveis;
- superfície IPC pequena e revisável;
- frontend pode evoluir sem conhecer schema ou material criptográfico.

Custos:

- DTOs e erros precisam de versionamento explícito;
- validação relevante aparece na UI e novamente no Rust;
- testes de integração command/caso de uso são necessários.

## Alternativas rejeitadas

- **SQLite diretamente no frontend:** amplia IPC, acopla UI ao schema e quebra a fronteira de autorização.
- **Regra de negócio em commands:** dificulta teste/reuso e mistura transporte com domínio.
- **Backend web local:** aumenta instalação, portas/processos e superfície operacional sem necessidade offline.
- **Toda regra em TypeScript:** WebView não é fronteira confiável para dados/arquivos privilegiados.

## Critérios de conformidade

- nenhum import de driver SQLite em `src/`;
- nenhum command aceita SQL ou path arbitrário sem escopo/canonicalização;
- testes provam autorização no Rust mesmo com payload IPC direto;
- chaves e senhas nunca aparecem na resposta serializada;
- commands públicos permanecem enumeráveis e mínimos.
