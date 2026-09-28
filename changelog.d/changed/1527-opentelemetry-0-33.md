- **Familia opentelemetry sobe para 0.33 numa unica mudanca (#1527, #1528, #1530, #1531).**
  `opentelemetry`, `opentelemetry_sdk` e `opentelemetry-otlp` vao de 0.32 para 0.33 e
  `tracing-opentelemetry` de 0.33 para 0.34 no `garraia-telemetry`. O Dependabot abriu
  uma PR por crate e as quatro ficaram vermelhas: as versoes sao acopladas e uma sozinha
  deixa duas versoes incompativeis do mesmo facade no grafo. Um grupo `opentelemetry` no
  `dependabot.yml` passa a casa-las por pattern em qualquer update-type, como ja era feito
  para o par `wasmtime`/`wasmtime-wasi` desde os PRs #833/#838.
