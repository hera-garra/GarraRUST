- **Presets nomeados de permissao no WhatsApp pessoal (#1434).** `garra whatsapp
  preset <numero> chat_only|read|developer|full_pod` e a acao `preset` da API admin
  gravam `level` e `write` de uma vez, pelo mesmo caminho unico de mutacao da Access
  Policy v2 (ADR 0025 §4bis): um preset e apelido para uma combinacao canonica, nao um
  campo novo no `config.yml` — nao ha rotulo guardado que possa divergir do que o portao
  aplica. `chat_only` nao libera ferramenta nenhuma e `read` so leitura, os dois seguros
  para contexto remoto; `developer` e `full_pod` compilam para o mesmo `full`, que por
  construcao nao tem teto proprio — quem decide e o modo mais o `execution.profile`
  (ADR 0024). O mesmo nome serve para o default do desconhecido e para os grupos —
  `garra whatsapp access default|groups default|group ... --preset <nome>` e o campo
  `preset` nas acoes `default`, `group-default` e `group` da API —, com as guardas de
  sempre: `developer` e `full_pod` sao `full` e continuam recusados em `access.default`.
  O documento de politica efetiva em JSON (`garra whatsapp access --json` e
  `GET /admin/api/whatsapp/access`) passa a trazer um rotulo `preset` calculado ao vivo
  do `level`/`write`, com `custom` para o que nenhum preset cobre.
