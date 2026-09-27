- **O gateway virou servidor MCP por HTTP, em `POST /mcp` (#1513).** O Garra ja
  falava MCP como cliente (secao `mcp:`) e como servidor por stdio (`garra
  mcp-server`, uma tool), mas nao a combinacao que um orquestrador externo
  precisa — servidor, por URL. Sem ela nao havia como apontar o Paperclip (ou
  qualquer host MCP que so aceite endereco) para este Garra. A ponte usa o
  `rmcp` que ja estava no workspace, e expoe cinco tools: `garra_status`,
  `garra_list_chats`, `garra_read_history` (com segredos redigidos pela mesma
  `redact_secrets` do log), `garra_pair_status` e `garra_send_message`.
  Transporte stateless com resposta em JSON, entao um `curl` confere o endpoint
  sem parsear SSE. Tudo desligado por default: `gateway.mcp_http.enabled` liga a
  rota, e ela **recusa subir sem `gateway.api_key`** — o gate de `/api/*` e
  passa-direto sem chave, o que serve para o console local e nao serve para uma
  ponte que entrega a lista de conversas do dono. O envio exige duas acoes
  independentes do operador (`allow_send` mais o destino em
  `proactive_chat_ids`), e uma tool que nao pode enviar nem e anunciada. O
  `garra config check` avisa nos dois erros silenciosos: ponte ligada sem
  credencial e envio ligado sem allowlist. `docs/gateway-mcp-http.md`.
