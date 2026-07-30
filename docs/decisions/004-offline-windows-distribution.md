# ADR 004 — Distribuição offline para Windows

- **Status:** substituída pelo [ADR 006](006-native-web-service.md)
- **Data:** 2026-07-22

## Contexto

O público-alvo pode instalar e operar sem internet. Um bootstrapper que baixa WebView2, autenticação remota, CDN, auto-update obrigatório ou dependência de servidor violaria esse requisito. Ao mesmo tempo, suportar sistemas operacionais sem correções de segurança expõe dados clínicos.

## Decisão

O primeiro alvo de distribuição será **Windows x64**:

- Windows 11 atualizado é o alvo preferencial;
- Windows 10 somente em 22H2 com ESU válido ou edição LTSC ainda suportada;
- Windows 10 fora dessas condições, x86, ARM e outros sistemas ficam fora do suporte;
- aplicação e fluxos essenciais não realizam requisição de rede;
- fonts/assets ficam no bundle, sem CDN;
- futuro instalador incorpora WebView2 Offline Installer ou runtime fixo;
- nenhuma telemetria ou atualização automática no MVP;
- atualização será manual, assinada, documentada e sempre preservará dados/backup.

O instalador offline maior é preferível a um bootstrapper pequeno que falha sem rede. A escolha entre MSI/NSIS e offline installer/fixed runtime será validada na etapa de release sem alterar o requisito de independência de rede.

## Pipeline atual

CI roda em `windows-2022` e verifica frontend, Rust, auditoria e licenças. Ele não:

- assina binários;
- gera/publica release;
- faz upload de instalador;
- usa certificado ou secrets;
- certifica funcionamento em todas as edições Windows suportadas.

Build do código não equivale a pacote distribuível.

## Gates de distribuição

Todos devem estar verdes:

1. licença do projeto definida e atribuições/SBOM revisadas;
2. SQLCipher ≥ 4.17 e `distributionReady=true`;
3. restore completo, rollback e recovery em máquina limpa;
4. assinatura de código e custódia/rotação do certificado;
5. instalador e WebView2 operando sem internet;
6. upgrade/downgrade suportado sem perda de dados;
7. testes em Windows 11 e nas variantes Windows 10 declaradas;
8. scanner de malware/supply chain e revisão de ações/dependências;
9. documentação operacional e suporte de incidente;
10. revisão de privacidade, retenção e uso de dados reais.

A fundação atual falha deliberadamente nos itens de SQLCipher, restore, assinatura, instalador e licença; portanto não há release.

## Consequências

Positivas:

- instalação e operação não dependem da conectividade da unidade;
- superfície de rede e custo operacional são menores;
- matriz de suporte explícita evita promessa insegura para Windows sem patches.

Custos:

- instalador offline cresce significativamente;
- atualizações exigem processo manual seguro;
- Windows 10 requer verificação de edição/ciclo/ESU pela organização;
- serão necessárias máquinas/VMs de teste além do runner CI.

## Alternativas rejeitadas

- **Bootstrapper WebView2 com download:** não funciona em instalação isolada.
- **PWA/browser:** reduz controle sobre chave, filesystem, backup e instalação offline.
- **Updater obrigatório:** cria dependência de internet e serviço externo.
- **Suportar qualquer Windows 10:** mantém dados sensíveis em SO sem correções.
- **Publicar artefato não assinado:** risco de cadeia de suprimentos e alertas ao usuário.

## Referências

- [Tauri — Windows Installer/WebView2](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options)
- [Windows 10 Lifecycle](https://learn.microsoft.com/en-us/lifecycle/products/windows-10-home-and-pro)
- [Windows 10 ESU](https://learn.microsoft.com/en-us/windows/whats-new/extended-security-updates)
- [Windows 10 Enterprise LTSC 2021](https://learn.microsoft.com/en-us/lifecycle/products/windows-10-enterprise-ltsc-2021)
