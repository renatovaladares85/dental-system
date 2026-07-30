# ADR 005 — Política de senha NIST e divergência do Figma

- **Status:** aceita
- **Data:** 2026-07-22

## Contexto

O aplicativo terá autenticação local por senha como fator único. Regras visuais comuns de “8 caracteres + maiúscula + minúscula + número + símbolo” estimulam padrões previsíveis e divergem das recomendações atuais. A copy do protótipo precisa ser revalidada no Figma, mas não pode reduzir o controle aprovado.

O NIST SP 800-63B-4 trata primariamente identidade digital em rede; será usado como baseline conservadora, sem alegar certificação ou conformidade formal do produto.

## Decisão

Política para criação, alteração e reset administrativo:

- mínimo de **15 caracteres** por ser senha de fator único;
- máximo de **128 code points Unicode**;
- aceitar espaços, ASCII imprimível e Unicode;
- aplicar normalização Unicode NFC antes de medir e derivar;
- não remover espaços, alterar caixa ou truncar silenciosamente;
- não exigir combinação de maiúscula, minúscula, número ou símbolo;
- comparar a senha integral com blocklist local de valores comuns, comprometidos e contextuais;
- rejeitar username, nome do produto/organização e variações óbvias quando constarem da blocklist;
- permitir colar e usar gerenciadores de senha;
- não exigir troca periódica; forçar troca quando houver evidência de comprometimento/reset;
- não usar dica, perguntas de segurança ou recuperação por conhecimento pessoal;
- erro de login genérico e limitação progressiva de tentativas.

Hash:

- Argon2id v19;
- `m=65536 KiB`, `t=3`, `p=1`;
- saída de 32 bytes;
- salt CSPRNG exclusivo de 16 bytes;
- string PHC completa persistida;
- rehash após login quando parâmetros evoluírem;
- hash fictício equivalente para username inexistente.

Parâmetros devem ser medidos no hardware mínimo, com uma derivação concorrente controlada. Redução futura requer benchmark e nova decisão, nunca mudança silenciosa.

### Estado da fundação

A criação do master normaliza em NFC no Rust, valida 15–128 code points, termos contextuais e a blocklist offline versionada `src-tauri/resources/common-passwords-v1.txt`, compartilhada com o frontend, e persiste Argon2id nos parâmetros acima. O login do `MASTER_ADMIN` usa hash fictício equivalente, resposta genérica, sessão opaca e rate limit em memória. Uma fonte offline abrangente de credenciais comprometidas, benchmark no hardware mínimo e perfis adicionais continuam pendentes; por isso a entrega ainda não está liberada para dados reais.

## Divergência visual

Se o Figma apresentar regra de oito caracteres/composição, a implementação e a copy devem ser substituídas pela política acima. O formulário deve orientar com linguagem simples, por exemplo:

> Use uma frase longa com pelo menos 15 caracteres. Espaços são permitidos; não exigimos símbolos específicos.

Não exibir checklist de classes de caractere como requisito. Um indicador de força só pode orientar e não substituir mínimo/blocklist. A revalidação final do frame permanece pendente por cota Figma.

## Consequências

Positivas:

- favorece frases longas e reduz padrões previsíveis;
- melhora compatibilidade com gerenciadores de senha e acessibilidade;
- elimina expiração periódica sem evidência;
- parâmetros/salt no PHC permitem atualização gradual.

Custos e riscos:

- 15 caracteres exigem copy clara e podem aumentar suporte inicial;
- Unicode requer mesma normalização no cadastro e login;
- blocklist precisa ser empacotada e atualizada sem consulta externa;
- Argon2id pode causar negação de serviço se tentativas/concor­rência não forem limitadas.

## Alternativas rejeitadas

- **8 caracteres com composição:** menor espaço efetivo e comportamento previsível.
- **Expiração a cada N dias:** incentiva alterações fracas sem evidência de comprometimento.
- **Validação por API de vazamentos:** quebra operação offline e pode expor metadados.
- **PBKDF2/bcrypt por conveniência:** Argon2id é a escolha aprovada e memory-hard.
- **Senha reversível:** proibida.

## Critérios de aceite

- testes contam code points após NFC, não bytes/unidades UTF-16;
- 14 caracteres falham e 15 passam quando fora da blocklist;
- 128 passam; 129 falham sem truncamento;
- espaços, acentos e colagem funcionam;
- composição específica não é exigida;
- login válido/ inválido tem resposta pública equivalente;
- senha/hash não aparecem em log, auditoria, backup aberto ou resposta HTTP;
- parâmetros PHC são verificados e rehash pode ser sinalizado.

## Referência

[NIST SP 800-63B-4 — Passwords](https://pages.nist.gov/800-63-4/sp800-63b.html#passwords)
