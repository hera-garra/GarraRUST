# Novidades da v0.4.6

> 🇬🇧 [English version](Whats-New-v0.4.6) · 📋 [CHANGELOG completo](https://github.com/michelbr84/GarraRUST/blob/main/CHANGELOG.md) · 📦 [Baixar](https://github.com/michelbr84/GarraRUST/releases/tag/v0.4.6)

Duas frentes grandes e uma faixa de estabilização.

A primeira é a **Access Policy v2** ([ADR 0025](https://github.com/michelbr84/GarraRUST/blob/main/docs/adr/0025-classes-de-capacidade-e-politica-de-acesso-v2.md)):
quem pode falar com o Garra pelo WhatsApp pessoal, e **até onde cada um vai**,
passa a ser um motor único que a CLI, a API admin e uma página nova do Web
Console compartilham — não três cópias da mesma regra, cada uma errando de um
jeito diferente.

A segunda é a **honestidade do runtime**: um registro único de capacidades passa
a responder "o que você consegue fazer?" para o `garra_status`, para o
`/api/diagnostics`, para o console e para o próprio modelo — com estado, motivo
e remediação. O modelo deixa de dizer "não tenho MCP" quando tem MCP escondido,
e "não existe" quando o certo é "existe e não está liberada".

Em volta disso: MCP `filesystem` confinado ao mesmo jail das file tools nativas,
circuit breaker para ferramenta que falha em série, o gateway virando servidor
MCP por HTTP, e o CI subindo sem depender de registry.

---

## Access Policy v2 — quem entra e até onde vai (#1388, ADR 0025)

`channels.whatsapp_linked.access` é a seção nova. Ela responde quatro perguntas
que antes viviam espalhadas entre `allow`, `owners` e `reply_in_groups`:

| Pergunta | Onde | Valores |
|---|---|---|
| Quem é admitido? | `access.admission` | `restricted` (só quem está na lista) · `open` (qualquer um) |
| O desconhecido entra com o quê? | `access.unknown_default` | o nível de partida de quem não está listado |
| Grupos participam? | `access.groups` | ligado/desligado e **política própria por grupo** |
| Cada pessoa vai até onde? | piso + teto por principal | `chat` · `read` · `full`, mais escrita de arquivo à parte |

O nível é um **teto que compõe com o modo da sessão: só tira, nunca põe.** Um
número em `chat` não ganha ferramenta porque a sessão está em modo `code` — a
política e o modo se cruzam no ponto mais restritivo. Quem já tinha
`allow`/`owners` no `config.yml` continua valendo: a v2 lê o legado, e o
`access.groups.enabled` declarado vence o `reply_in_groups` antigo (#1501).
Grupos ligados e desligados valem **na mensagem seguinte**, sem restart (#1412).

### Pela CLI

```bash
garraia whatsapp access                      # a politica EFETIVA, pelo motor do turno
garraia whatsapp users --json                # quem esta autorizado, por papel
garraia whatsapp level +5511999998888 read   # teto de um numero: chat | read | full
garraia whatsapp level +5511999998888 read --dry-run   # o impacto, sem gravar
garraia whatsapp write +5511999998888 on     # escrita de arquivo, e SO ela
garraia whatsapp block +5511999998888        # vence 'open', 'allow' e pareamento
garraia whatsapp unblock +5511999998888
garraia whatsapp owner +5511999998888        # promove a dono (so em isolated-pod)
garraia whatsapp unowner +5511999998888      # rebaixa SEM revogar o acesso
garraia whatsapp remove +5511999998888       # revoga
```

Identidade aparece **só pelos quatro últimos dígitos** em toda tela e em todo
`--json` — o número inteiro fica no `config.yml` (modo `0600`). `--reveal`
mostra os valores da config, localmente. Todo comando de permissão deixa rastro
no audit, inclusive os legados `allow`/`remove`/`owner`/`unowner` (#1414).

### Pelo Web Console

A página **WhatsApp Access** (#1402–#1405) é a mesma política pela mesma API:
matriz por principal, **preview do efeito antes de confirmar**, e o telefone
validado pela mesma regra da CLI (#1403) — não uma segunda validação que aceita
o que a CLI recusa. A matriz mostra web e memória junto das demais capacidades
(#1411). O botão **Test WhatsApp** roda o **mesmo motor** do
`garraia doctor whatsapp` (#1420), e as rejeições recentes ficam visíveis
(#1422).

### Diagnóstico de ponta a ponta

```bash
garraia doctor whatsapp            # ou --json
```

Percorre o caminho inteiro (#1419): vínculo, chave da sessão, gateway, política
de acesso, perfil de execução, workspace, visibilidade do MCP e provedor. Exit 0
quando passa, 69 quando algo está indisponível — com a linha que diz o quê.

### O wizard fecha mostrando o portão

`garraia whatsapp link` terminava com "pronto para receber mensagens" sem dizer
quem, afinal, podia falar. Agora ele imprime o mesmo resumo do
`garraia whatsapp access` — canal, perfil de execução, admissão, default do
desconhecido, grupos e cada principal com piso, nível e o que pode de fato
(#1429). E o `garra init` passa a oferecer o WhatsApp ao lado do Telegram
(#1430).

---

## Honestidade do runtime: registro de capacidades (#1381, #1387, #1416)

Uma função só (`capacidades_registro::registro`) reúne o inventário do runtime,
o portão do turno, a disponibilidade de cada ferramenta, o estado dos servidores
MCP e a exposição do `bash`. Cada capacidade sai com **estado**, motivo legível
por máquina e por humano, e remediação:

| Estado | Significa |
|---|---|
| `visible` | existe e está liberada |
| `denied` | **existe e não está liberada** — nunca "não existe" |
| `unavailable` | existe, não dá para usar agora (a remediação diz por quê) |
| `unhealthy` | servidor registrado, fora do ar |
| `not_configured` | nunca foi configurada — diferente de quebrada |

O mesmo registro alimenta `garra_status` (que agora devolve `capabilities`), o
`/api/diagnostics` (`tools.capabilities`) e `GET /admin/api/capabilities`. A
nota do prompt manda o modelo **responder a partir dele** e o proíbe de inferir
ausência a partir de invisibilidade. O registro também diz quando falta
contexto — se as file tools têm raiz nesta sessão, e qual repositório está ativo
(#1416, #1418) — e `garra_status` explica que `withheld` é campo retido por
política, não capacidade ausente (#1382).

Ferramenta registrada mas **não operacional sai da lista chamável** em vez de
ser oferecida ao modelo e falhar na chamada (#1425).

### Classes de capacidade e teto por principal (#1385, #1392)

Toda ferramenta declara o que faz em classes fechadas — `filesystem.read`,
`filesystem.write`, `process.execute`, `network.read`, `message.send`,
`device.*`, `memory.*`, `runtime.inspect`, `scheduling`, `mcp.read`,
`mcp.write`. Nativas por tabela fechada; MCP pela operação de filesystem
conhecida ou pelas anotações do servidor. **Sem classe declarada, modo restrito
falha fechado.** O teto do principal entra no `ExecContext` e compõe com o modo.

---

## Segurança

- **MCP `filesystem` confinado ao jail da sessão** (#1482, #1383). Havia dois
  portões com regras diferentes no mesmo turno: as file tools nativas
  respeitavam o jail por sessão, e as chamadas MCP de filesystem não. Agora um
  só jail por sessão governa as duas.
- **Ferramenta MCP não vira mais slash command** (#1386). O registro automático
  expunha toda tool MCP como `/comando`, **fora do `ToolGate`** dos modos — um
  bypass da política de ferramentas.
- **Sessão por id separada em cliente e operador** (#1462). Ids de canal são
  adivinháveis por construção (`whatsapp-linked-<número>`, `telegram-<chat>`).
  A escrita já havia sido fechada; agora a **leitura** também: `GET
  /api/sessions`, `GET /api/sessions/{id}/history`, `DELETE
  /api/sessions/{id}` e o `resume` sem token do `/ws` só alcançam as
  superfícies locais do operador (`api`, `vscode`, `web`, `parrot`). Sessão de
  canal ou do mobile responde o mesmo `404 session not found` de id
  inexistente, byte a byte, e não é hidratada, desconectada nem listada. A
  leitura do operador passou para `GET /admin/api/sessions/{id}/history`
  (cookie do `/admin` + `manage_sessions`; `viewer` recebe 403).
- **`/api/diagnostics` não publica caminho absoluto do host** (#1465).
- **Telefone com separador comum agora é mascarado** (#1514) — o mascaramento
  cobria o número cru e passava batido em `+55 11 99999-8888`.
- **Workspace padrão e diretório de sessão nascem `0700`** (#1463).

## Workspace e projeto por sessão

- **Sessão sem projeto ganha workspace seguro isolado por sessão** (#1378,
  #1449). Com `agent.file_roots` vazio — o default — o jail ficava *sem raiz*, e
  sem raiz nega tudo: as file tools apareciam registradas e toda chamada voltava
  `NoRoots`. Agora a raiz efetiva é `<data_dir>/workspace/<sessão>` (nome =
  SHA-256 do `session_id`, nunca o id em claro). Nunca `/`, nunca `$HOME`, nunca
  o `pod_root`. O "por sessão" vem da revisão independente (#1449): a primeira
  versão tinha raiz compartilhada, e um contato do WhatsApp poderia ler o que
  outro escreveu. `agent.file_roots` declarado continua vencendo sozinho.
- **`/project` seleciona um projeto por sessão** (#1379, #1424). O caminho é
  reconfinado pelas raízes de projeto do operador (`GARRAIA_PROJECT_ROOTS`),
  vira o `working_dir` das file tools e do `repo_search`, e o vínculo persiste no
  `sessions.db` — um `garraia restart` restaura o projeto, e um projeto que saiu
  das raízes **não volta** (fail-closed). A resposta mostra nome e id curto,
  nunca o caminho. No WhatsApp, mensagem que começa com `/` é decidida **por
  principal antes de ir ao modelo**: `/help` para todo admitido, `/project` para
  dono e usuário, `/mode` e `/goal` só para o dono. Selecionar projeto **não
  muda poder** — o portão do turno continua sendo modo + teto.
- **Raízes resolvidas uma vez, no boot** (#1459).

## Confiabilidade

- **Circuit breaker por sessão e ferramenta** (#1417), no único ponto de
  despacho: falha determinística pausa a ferramenta até o fim do turno, timeout
  abre cooldown com backoff exponencial 15s→120s, e erro genérico só depois de
  três iguais. Vem do dogfood da v0.4.5, onde o modelo repetiu `repo_search`
  várias vezes no mesmo turno.
- **Registro local de confiabilidade** de tools, MCP e canais (#1438) — nada sai
  da máquina.
- **`repo_search` falha rápido** sem repositório ativo (#1380).
- **`garraia stop` não trata mais um zumbi como processo vivo** (#1426).

## Superfícies novas

- **O gateway virou servidor MCP por HTTP, em `POST /mcp`** (#1513, Streamable
  HTTP) — além do `garra mcp-server` stdio que já existia.
- **`garra mcp-server` volta a entregar as tools ao Claude Code** (#1518): desde
  a spec `2026-07-28` (SEP-2549) o `tools/list` exige `ttlMs` e `cacheScope`.
- **Retenção de memória e do ledger: fatia de dados** (#1436) —
  `garraia-config::retention` + `garraia-db::retention`.
- **Modo efetivo por sessão** (#1409) e **badge do perfil de execução** no
  cabeçalho do console (#1410).
- **Modo `search` enxerga a leitura do MCP `filesystem`** (#1384) — write, edit
  e delete seguem negados.

## Diagnóstico: instalação limpa não nasce amarela

- O `/api/diagnostics` separa **"nunca configurado"** de **"quebrado"** (#1437):
  TLS ausente, `.env` inexistente, Telegram sem token ou WhatsApp sem aparelho
  vinculado deixam de acender amarelo numa instalação nova.
- E os dois últimos avisos espúrios saíram (#1471): `tools.bash` com
  `agent.sandbox.mode = off` — o default documentado do perfil `standard` — e
  `runtime.channels` sem canal de mensageria passam de `warning` a
  `not_configured`, com passo de remediação correto. Sandbox **configurado e
  inutilizável** continua `warning`.

## Processo e ferramentas

- **Gate de dogfood D1–D10 obrigatório antes de todo tag**
  ([`docs/releasing.md`](https://github.com/michelbr84/GarraRUST/blob/main/docs/releasing.md) §1.5,
  #1439). A v0.4.4 e a v0.4.5 saíram verdes no CI e quebraram no caminho real; o
  CI prova que compila, não que uma instalação limpa funciona para uma pessoa. A
  matriz é executada **contra o candidato**, com data, sistema e executor.
  **Linha sem data, release não sai.**
- **A parte Linux de D1/D9 está automatizada** (#1426):
  `scripts/dogfood/linux-clean-install.sh` instala o `.deb` num `ubuntu:24.04`
  cru, roda os doctors, aponta para um Ollama local, exige **resposta real do
  modelo** por REST, reinicia e prova que sessão e config sobreviveram.
- **CI sobe o MinIO sem registry** (#1458). Em 2026-09-24 o `quay.io` passou a
  exigir login para toda tag de `minio/minio` — depois de o Docker Hub remover o
  repositório e de o `dl.min.io` responder 410. O que segue público é o
  **fonte**: `scripts/ci/build-minio-image.sh` compila a tag upstream fixada
  (recusando se ela não apontar mais para o commit esperado) e etiqueta a imagem
  que o testcontainer procura.
- O hook `pre-tool-use` deixa de bloquear `rm -rf /tmp/…` e `rm -rf ./x`: o
  bloqueio é ancorado no **alvo** (raiz, home, diretório atual, pai), não no
  prefixo (#1453).
- `scripts/setup-toolchain.sh` fixa a toolchain localmente por `rustup
  override`, sem `rust-toolchain.toml` — que quebra o cross-compile do CI
  (#1452).
- iMessage: o campo da sala chama-se `room_id` (#1461).

## Atualizando da v0.4.5

```bash
garraia update
```

Nada muda de formato: `config.yml`, `session.enc` do WhatsApp, `allow`/`owners`
e o histórico continuam valendo. Três mudanças de comportamento visíveis:

- Sessões remotas sem projeto passam a ter arquivos em
  `<data_dir>/workspace/<sessão>` — uma pasta nova, vazia, por sessão. Quem já
  declarava `agent.file_roots` não vê diferença.
- Clientes que usavam `X-Session-Id` (ou o id na URL) para **alcançar uma sessão
  de canal** pela API — escrever nela, listá-la ou ler o histórico — passam a
  receber `404`. Era exatamente o caminho que a #1462 fecha. A conversa continua
  acessível pelo próprio canal, e o operador lê qualquer sessão por
  `GET /admin/api/sessions/{id}/history`.
- Um servidor MCP não registra mais slash commands (#1386). Quem chamava uma
  tool MCP por `/nome` passa a chamá-la como ferramenta, sob a política do modo.

## Limites conhecidos

- O QR do WhatsApp continua **no terminal**: levá-lo para o aplicativo desktop é
  o [plan 0363](https://github.com/michelbr84/GarraRUST/blob/main/plans/0363-desktop-whatsapp-sem-terminal.md),
  não esta release.
- A página **global** de Agents & Permissions (#1433), os **presets** de
  permissão (#1434) e o **import/export** de políticas (#1435) ficam para o
  ciclo seguinte: a v2 provou o padrão numa página; generalizá-la é decisão de
  escopo.
- Instaladores do Windows e bundles de desktop saem **não assinados** — não há
  certificado de assinatura nem Apple Developer ID no projeto.
- O auto-updater do desktop Tauri segue desativado por desenho; o caminho de
  atualização suportado é o `garra update` da CLI
  ([runbook](https://github.com/michelbr84/GarraRUST/blob/main/docs/releasing.md)).
