- **`utoipa` 6.0 e `utoipa-swagger-ui` 10.0 sobem juntos (#1526, #1529).** As duas
  crates trocam o tipo `utoipa::openapi::OpenApi` entre si, entao bumpar so uma
  quebra a compilacao: a `utoipa-swagger-ui` 9.0.2 declara
  `SwaggerUi::url(self, url, openapi: OpenApi)` contra a serie 5.x, e com a
  `utoipa` em 6.0.0 o gateway parava num `E0308: mismatched types` — dois
  facades incompativeis no mesmo grafo. A `utoipa-swagger-ui` 10.0.0 foi
  re-lancada justamente para casar com a `utoipa` 6.0, entao o par so anda em
  conjunto. Nenhuma mudanca de codigo foi necessaria: as breaking changes da
  `utoipa` 6 (fim do ignore por expressao em `ToSchema`/`IntoParams`,
  `utoipa-gen` sobre `syn` 3) nao tocam nada que o gateway use. O build offline
  do Swagger UI segue pela feature `vendored`, agora via
  `utoipa-swagger-ui-vendored` 0.2. Para nao repetir o bump partido, o
  `.github/dependabot.yml` ganhou um grupo `utoipa`, no mesmo molde
  dos grupos `wasmtime` e `opentelemetry`.
