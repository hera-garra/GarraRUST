- **`garra mcp-server` volta a entregar as tools ao Claude Code (#1518).** Quem
  negocia a spec MCP `2026-07-28` (o Claude Code, via `server/discover`) valida
  o `tools/list` contra o schema novo, que torna `ttlMs` e `cacheScope`
  obrigatorios (SEP-2549). O rmcp 3.x (#1162) modela os dois como `Option` e o
  `ListToolsResult::with_all_items` os deixa fora do fio, entao o cliente
  recusava a lista inteira ("Invalid result for tools/list") e o `/mcp`
  mostrava o servidor conectado, mas sem nenhuma tool. O handler agora manda
  `ttlMs: 0` e `cacheScope: "private"` quando a versao negociada e
  `2026-07-28` ou mais nova; peer legado segue recebendo o `tools/list` de
  antes, byte a byte.
