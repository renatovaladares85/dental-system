# Mapeamento do Figma

## Estado da descoberta

O protótipo **Offline Dental System Prototype** foi inspecionado anteriormente e permitiu extrair telas, componentes e tokens abaixo. A revalidação visual final e a comparação frame a frame estão pendentes porque a cota da integração Figma foi atingida.

Consequências:

- o inventário serve para estruturar a fundação visual;
- fidelidade pixel a pixel, variantes, assets e estados não podem ser declarados concluídos;
- divergências encontradas na revalidação devem ser registradas antes de mudar fluxo funcional;
- requisitos funcionais e segurança prevalecem sobre o mock.

O material gerado pelo Figma é um mock monolítico em React 19/Tailwind 4. Ele não deve ser copiado como arquitetura da aplicação.

## Telas identificadas

| Área/tela      | Papel no produto                    | Estado nesta fundação                            |
| -------------- | ----------------------------------- | ------------------------------------------------ |
| login          | entrada local e mensagens genéricas | login real do `MASTER_ADMIN` integrado           |
| setup          | primeiro acesso em cinco passos     | integração com setup/recovery inicial            |
| lock           | bloqueio de sessão                  | sessão real existe; tela de lock dedicada futura |
| dashboard      | resumo operacional                  | visual sem indicadores clínicos reais            |
| patients       | lista/pesquisa                      | fora desta fundação                              |
| patient-new    | cadastro                            | fora desta fundação                              |
| patient-detail | visão clínica/administrativa        | fora desta fundação                              |
| treatments     | fluxo de tratamentos                | fora desta fundação                              |
| catalog        | catálogo de procedimentos           | fora desta fundação                              |
| plan           | plano de tratamento                 | fora desta fundação                              |
| financial      | mock visual sem regras aprovadas    | explicitamente fora do MVP atual                 |
| documents      | documentos/anexos                   | regra e segurança ainda precisam ser definidas   |
| users          | usuários e papéis                   | visual/futuro; RBAC real pendente                |
| audit          | trilha de eventos                   | cobertura completa pendente                      |
| backup         | continuidade e status               | backup inicial integrado; gestão/restore futuros |
| settings       | organização e preferências          | fora desta fundação, salvo setup mínimo          |

Não foram confirmados frames separados de agenda, prontuário completo ou odontograma nesta extração. Eles continuam requisitos do MVP, mas precisam de contexto visual específico antes de implementação.

## Setup em cinco passos

| Passo | Conteúdo observado         | Integração esperada                                                      |
| ----- | -------------------------- | ------------------------------------------------------------------------ |
| 1     | boas-vindas ou recuperação | escolher instalação nova; restore completo permanece indisponível        |
| 2     | clínica e unidade          | validar dados organizacionais mínimos                                    |
| 3     | administrador master       | aplicar política NIST e criar hash Argon2id                              |
| 4     | geração de segurança       | gerar banco, DPAPI, `.odskey` e `.odsbackup`; retomada pode trocar mídia |
| 5     | confirmação                | redigitar código, validar arquivos distintos e concluir setup por último |

A UI não deve sinalizar sucesso antes da validação criptográfica real. O caminho de “recuperar” deve explicar que restauração integral ainda não está implementada, sem simular conclusão.

## Componentes identificados

- `Button` com variantes e estados disabled/loading;
- `Badge` e `StatusBadge`;
- `Input`, `Select` e `Textarea`;
- `Modal`;
- `Card`;
- `PageHeader`;
- `AlertBanner`;
- `Toast`;
- `Table`;
- `Pagination`;
- `Tabs`;
- `Drawer`;
- estados `Empty` e `Loading`;
- shell com sidebar e topbar.

Nesta fundação já existem `AppLogo`, `Button`, `Card`, `Field`, `Checkbox` e `Alert`, além de `SetupLayout` e os cinco passos em `src/features/setup/components/`. Modal, badges, toast, tabela, paginação, tabs, drawer e estados genéricos continuam no inventário, não implementados.

Componentes novos devem permanecer acessíveis e pequenos, sem reproduzir o arquivo monolítico. Modal/Drawer devem controlar foco, fechar por teclado quando seguro e restaurar foco ao elemento de origem. Tabelas precisam de cabeçalhos semânticos, navegação por teclado e estado vazio.

## Tokens extraídos

### Tipografia

- interface: **Inter**;
- dados técnicos/IDs: **JetBrains Mono**.

O protótipo carrega Google Fonts remotamente, o que é incompatível com operação offline e CSP restritiva. A fundação substitui isso pelos pacotes locais `@fontsource-variable/inter` e `@fontsource-variable/jetbrains-mono`, com fallback de sistema e sem requisição externa. Licenças/atribuições entram no gate de distribuição.

### Cor primária `petrol`

| Token        | Valor     |
| ------------ | --------- |
| `petrol-50`  | `#f0f8fb` |
| `petrol-100` | `#d0ecf5` |
| `petrol-200` | `#a0d8ea` |
| `petrol-300` | `#62bdd9` |
| `petrol-400` | `#2ea2c6` |
| `petrol-500` | `#1d8bae` |
| `petrol-600` | `#1a7393` |
| `petrol-700` | `#195e78` |
| `petrol-800` | `#1a4d62` |
| `petrol-900` | `#1b4153` |
| `petrol-950` | `#0d2535` |

Não derivar cores semânticas de sucesso/alerta/erro sem revalidar contraste e estados no arquivo.

Os tokens implementados estão centralizados em `src/styles/index.css`; componentes não devem duplicar a paleta em constantes TypeScript.

### Espaçamento, raio e viewport

- escala base de espaçamento: 4 px e 8 px;
- raios principais: 8 px e 12 px;
- frame de referência: 1440 × 900;
- resolução mínima exigida: 1366 × 768;
- setup administrativo mantém referência desktop; login e shell de indisponibilidade precisam funcionar nos navegadores móveis suportados.

Evitar posicionamento absoluto como estratégia principal. Shell, tabelas e formulários devem manter zoom e viewport mínimo sem ocultar ações críticas.

## Estados obrigatórios

Para cada tela aplicável, mapear durante a revalidação:

- inicial/carregando;
- vazio;
- conteúdo;
- validação de campo;
- sucesso;
- erro recuperável;
- sem permissão;
- operação sensível/confirmada;
- servidor indisponível — shell informativo, sem dados ou edição offline.

Dados exibidos no mock são ilustrativos. Não devem virar fixture de produção nem regra de negócio.

## Divergências registradas

| Tema        | Protótipo                                | Decisão do produto                                                                                               |
| ----------- | ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| arquitetura | React monolítico/Tailwind gerado         | módulos e componentes reutilizáveis; privilégio no Rust                                                          |
| fontes      | Google Fonts remota                      | `@fontsource` local + fallback, sem rede                                                                         |
| senha       | copy visual deve ser comparada novamente | política NIST de 15–128 caracteres, sem composição obrigatória; [ADR 005](decisions/005-nist-password-policy.md) |
| financeiro  | tela presente                            | sem regras aprovadas; não implementar                                                                            |
| recovery    | fluxo visual inclui recuperar            | somente geração/validação inicial nesta fundação; restore completo futuro                                        |
| plataforma  | shell originalmente desktop              | serviço web local; host faz setup e navegadores LAN usam HTTPS após pareamento                                   |

## Checklist para concluir a revalidação

- [ ] renovar cota/acesso e registrar URL + IDs dos frames principais;
- [ ] comparar screenshots em 1440 × 900 e 1366 × 768;
- [ ] verificar medidas, grid, tipografia, contraste, foco e zoom;
- [ ] confirmar variantes/estados de todos os componentes;
- [ ] inventariar ícones e assets com licença e arquivo local;
- [ ] confirmar agenda, prontuário e odontograma ou solicitar frames;
- [ ] substituir copy de senha incompatível com o ADR 005;
- [ ] validar setup de cinco passos contra operações reais do Rust;
- [ ] registrar divergências remanescentes neste documento.

Até concluir o checklist, a interface deve ser descrita como **fundação baseada no inventário Figma**, não como implementação visual final.
