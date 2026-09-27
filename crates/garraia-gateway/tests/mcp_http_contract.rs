//! #1513 — o contrato da ponte MCP Streamable HTTP, no `build_router` de verdade.
//!
//! Os testes de unidade em `src/mcp_http/` provam as decisoes (a politica de
//! envio, a superficie anunciada, a precondicao de credencial). Só isto aqui
//! prova a **montagem**: que `/mcp` nasce por dentro do gate de
//! `gateway.api_key`, que o `initialize → tools/list → tools/call` responde
//! JSON-RPC valido, e que um `garra_send_message` sem aprovacao do operador nao
//! envia.
//!
//! Sao os criterios 1, 2 e 3 da issue, na ordem:
//!
//! 1. `POST /mcp` responde JSON-RPC com a chave e **401** sem ela.
//! 2. Um host MCP enxerga as tools em `tools/list` — o que o Paperclip faz ao
//!    apontar para `http://127.0.0.1:3888/mcp`.
//! 3. `garra_send_message` sem aprovacao nao envia (e nem aparece na lista).
//!
//! Zero Docker, zero rede, zero Postgres: `AppState::new` + `build_router`, o
//! mesmo molde de `origin_guard_layering.rs`. O que este arquivo NAO cobre e o
//! caminho de entrega de verdade (um bot do Telegram do outro lado) — o teste
//! mais fundo que da para escrever aqui prova que o portao **abriu**, e a
//! recusa passou a ser "canal fora do ar" em vez de "nao autorizado".

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, Response, StatusCode};
use garraia_agents::AgentRuntime;
use garraia_channels::ChannelRegistry;
use garraia_config::AppConfig;
use garraia_gateway::admin::store::AdminStore;
use garraia_gateway::push_channels::PushChannelStates;
use garraia_gateway::router::build_router;
use garraia_gateway::state::AppState;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tower::ServiceExt;

const CHAVE: &str = "chave-de-teste-da-ponte-mcp";
const BEARER: &str = "Bearer chave-de-teste-da-ponte-mcp";

/// O `Accept` que o transporte Streamable HTTP exige (rmcp responde 406 sem os
/// dois tipos). Um cliente MCP manda isto; e parte do contrato, nao detalhe.
const ACCEPT: &str = "application/json, text/event-stream";

/// Um chat de destino qualquer. Vira allowlist quando o teste quer o portao
/// aberto.
const DESTINO: i64 = -100_123_456;

/// Como o operador liga a ponte.
#[derive(Clone, Copy, Default)]
struct Ligacao {
    enabled: bool,
    com_credencial: bool,
    allow_send: bool,
    /// Se um destino entra em `proactive_chat_ids` de um canal telegram.
    com_allowlist: bool,
}

impl Ligacao {
    /// O default da instalacao: nada ligado.
    fn desligada() -> Self {
        Self::default()
    }

    /// A ponte no ar, somente leitura — o caso de uso comum.
    fn leitura() -> Self {
        Self {
            enabled: true,
            com_credencial: true,
            ..Self::default()
        }
    }

    fn com(mut self, allow_send: bool, com_allowlist: bool) -> Self {
        self.allow_send = allow_send;
        self.com_allowlist = com_allowlist;
        self
    }
}

fn config_de(l: Ligacao) -> AppConfig {
    let mut config = AppConfig::default();
    config.gateway.mcp_http.enabled = l.enabled;
    config.gateway.mcp_http.allow_send = l.allow_send;
    if l.com_credencial {
        config.gateway.api_key = Some(CHAVE.to_string());
    }
    if l.com_allowlist {
        config.channels.insert(
            "tg".to_string(),
            garraia_config::ChannelConfig {
                channel_type: "telegram".to_string(),
                enabled: Some(true),
                settings: [("proactive_chat_ids".to_string(), json!([DESTINO]))]
                    .into_iter()
                    .collect(),
            },
        );
    }
    config
}

/// Um `POST /mcp` no router de verdade.
///
/// O `ConnectInfo` e inserido porque o rate limiter (por fora do gate, de
/// proposito) le o IP do par: sem ele o pedido morre em 500 no governor antes
/// de qualquer camada de auth — o mesmo comentario de `api_key_gate.rs` e
/// `origin_guard_layering.rs`.
async fn post_mcp(l: Ligacao, bearer: Option<&str>, corpo: Value) -> Response<Body> {
    let state = Arc::new(AppState::new(
        config_de(l),
        Arc::new(AgentRuntime::new()),
        ChannelRegistry::new(),
    ));
    let admin_store = Arc::new(Mutex::new(
        AdminStore::in_memory().expect("in-memory admin store"),
    ));
    let router = build_router(
        state,
        PushChannelStates::empty(),
        admin_store,
        Arc::new(vec![0u8; 32]),
    );

    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("accept", ACCEPT)
        .header("content-type", "application/json")
        // Loopback: o `allowed_hosts` default do rmcp so aceita loopback, e essa
        // e a propriedade que o teste tambem esta exercitando.
        .header("host", "127.0.0.1:3888");
    if let Some(b) = bearer {
        builder = builder.header("authorization", b);
    }
    let mut req = builder
        .body(Body::from(corpo.to_string()))
        .expect("request");
    req.extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            40404,
        ))));
    router.oneshot(req).await.expect("resposta")
}

async fn corpo_json(resp: Response<Body>) -> Value {
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("corpo");
    let texto = String::from_utf8_lossy(&bytes);
    // `json_response: true` da `application/json` no caminho simples; o fallback
    // para SSE existe no rmcp e este `strip_prefix` o cobre em vez de fingir que
    // nao existe.
    let cru = texto
        .lines()
        .find_map(|l| l.strip_prefix("data: "))
        .unwrap_or(&texto);
    serde_json::from_str(cru).unwrap_or_else(|e| panic!("resposta nao e JSON ({e}): {texto}"))
}

fn rpc(id: u32, metodo: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": metodo, "params": params })
}

/// O `initialize` que um host MCP manda primeiro.
fn initialize() -> Value {
    rpc(
        1,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "teste-de-contrato", "version": "0" }
        }),
    )
}

/// `initialize` + `tools/list` num router recem-montado.
///
/// Stateless de proposito (ver `mcp_http::configuracao_do_transporte`): sem
/// sessao para carregar entre pedidos, `tools/list` vale sozinho. O `initialize`
/// ainda roda antes porque e o que um host de verdade faz, e porque e metade do
/// criterio 1.
async fn tools_list(l: Ligacao) -> Value {
    let init = corpo_json(post_mcp(l, Some(BEARER), initialize()).await).await;
    assert_eq!(init["jsonrpc"], "2.0", "initialize: {init}");
    assert!(
        init["result"]["capabilities"]["tools"].is_object(),
        "initialize sem a capability tools — o host nunca chamaria tools/list: {init}"
    );
    corpo_json(post_mcp(l, Some(BEARER), rpc(2, "tools/list", json!({}))).await).await
}

fn nomes_das_tools(lista: &Value) -> Vec<String> {
    lista["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list sem array de tools: {lista}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// O envelope `garra.mcp.v1` de dentro do `CallToolResult`.
fn envelope(resposta: &Value) -> Value {
    let texto = resposta["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call sem conteudo de texto: {resposta}"));
    serde_json::from_str(texto).unwrap_or_else(|e| panic!("envelope nao e JSON ({e}): {texto}"))
}

async fn chamar(l: Ligacao, nome: &str, args: Value) -> Value {
    corpo_json(
        post_mcp(
            l,
            Some(BEARER),
            rpc(3, "tools/call", json!({ "name": nome, "arguments": args })),
        )
        .await,
    )
    .await
}

// ── criterio 1: a rota existe, e exige a chave ───────────────────────────────

/// No default da instalacao a ponte nao existe. **404, nao 401**: um 401 diria
/// "existe algo aqui, traga credencial", e nao existe.
#[tokio::test]
async fn desligada_por_default_a_rota_nao_existe() {
    let resp = post_mcp(Ligacao::desligada(), None, initialize()).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// A trava do `decidir_montagem`, agora no router: pedir a ponte sem configurar
/// `gateway.api_key` **nao** a abre. Sem isto, um operador que ligasse a ponte
/// num gateway sem chave publicaria a lista de conversas e o historico delas
/// para qualquer um que alcance a porta.
#[tokio::test]
async fn ligada_sem_credencial_a_rota_nao_sobe() {
    let l = Ligacao {
        enabled: true,
        com_credencial: false,
        ..Ligacao::default()
    };
    let resp = post_mcp(l, None, initialize()).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// Criterio 1, metade negativa: com a ponte no ar e sem bearer, 401 — e o 401 e
/// o do gate do gateway, nao um erro do rmcp.
#[tokio::test]
async fn sem_bearer_a_ponte_responde_401() {
    let resp = post_mcp(Ligacao::leitura(), None, initialize()).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("corpo");
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        "gateway: invalid or missing api key"
    );
}

/// Chave errada tambem e 401 — o gate compara por igualdade, nao por presenca.
#[tokio::test]
async fn bearer_errado_responde_401() {
    let resp = post_mcp(Ligacao::leitura(), Some("Bearer outra-chave"), initialize()).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Criterio 1, metade positiva: com a chave, o `initialize` responde JSON-RPC
/// valido e anuncia a capability `tools`.
#[tokio::test]
async fn com_bearer_o_initialize_responde_json_rpc() {
    let resp = post_mcp(Ligacao::leitura(), Some(BEARER), initialize()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = corpo_json(resp).await;
    assert_eq!(body["jsonrpc"], "2.0", "{body}");
    assert_eq!(body["id"], 1, "{body}");
    assert!(body["error"].is_null(), "{body}");
    assert!(
        body["result"]["protocolVersion"].is_string(),
        "initialize sem protocolVersion: {body}"
    );
    assert!(
        body["result"]["capabilities"]["tools"].is_object(),
        "{body}"
    );
}

// ── criterio 2: o host enxerga as tools ──────────────────────────────────────

/// O que o Paperclip ve ao apontar para `http://127.0.0.1:3888/mcp` numa
/// instalacao de leitura: as quatro tools de leitura da spec, e nenhuma de
/// escrita.
#[tokio::test]
async fn tools_list_mostra_as_quatro_de_leitura() {
    let nomes = nomes_das_tools(&tools_list(Ligacao::leitura()).await);
    for esperada in [
        "garra_status",
        "garra_list_chats",
        "garra_read_history",
        "garra_pair_status",
    ] {
        assert!(
            nomes.iter().any(|n| n == esperada),
            "falta {esperada}: {nomes:?}"
        );
    }
    assert_eq!(nomes.len(), 4, "{nomes:?}");
}

/// Com o operador tendo ligado o envio E liberado um destino, sao as cinco da
/// spec.
#[tokio::test]
async fn tools_list_mostra_as_cinco_quando_o_envio_esta_liberado() {
    let nomes = nomes_das_tools(&tools_list(Ligacao::leitura().com(true, true)).await);
    assert_eq!(nomes.len(), 5, "{nomes:?}");
    assert!(nomes.iter().any(|n| n == "garra_send_message"), "{nomes:?}");
}

/// `tools/call garra_status` de ponta a ponta: JSON-RPC valido por fora,
/// envelope `garra.mcp.v1` por dentro.
#[tokio::test]
async fn tools_call_garra_status_responde_o_envelope() {
    let resp = chamar(Ligacao::leitura(), "garra_status", json!({})).await;
    assert_eq!(resp["jsonrpc"], "2.0", "{resp}");
    assert!(resp["error"].is_null(), "{resp}");
    let env = envelope(&resp);
    assert_eq!(env["schema"], "garra.mcp.v1", "{env}");
    assert_eq!(env["ok"], true, "{env}");
    assert!(env["version"].is_string(), "{env}");
    assert!(env["channels"].is_array(), "{env}");
    // A propria ponte diz que nao pode enviar — e assim que o orquestrador
    // descobre isso sem tentar um envio.
    assert_eq!(env["mcp_http"]["send_enabled"], false, "{env}");
}

/// `garra_read_history` devolve o envelope mesmo para uma conversa que nao
/// existia: ela e criada vazia pela hidratacao, e "vazia" e uma resposta
/// honesta. O que importa aqui e o contrato — `redacted: true` sempre.
#[tokio::test]
async fn tools_call_read_history_declara_a_redacao() {
    let resp = chamar(
        Ligacao::leitura(),
        "garra_read_history",
        json!({ "chat": "conversa-que-nao-existe" }),
    )
    .await;
    let env = envelope(&resp);
    assert_eq!(env["ok"], true, "{env}");
    assert_eq!(env["redacted"], true, "{env}");
    assert_eq!(env["messages"], json!([]), "{env}");
}

/// Argumento invalido e erro de **protocolo** (bug do chamador), nao resultado
/// de tool: o `deny_unknown_fields` chega ao JSON-RPC como `error`.
#[tokio::test]
async fn argumento_desconhecido_vira_erro_de_protocolo() {
    let resp = chamar(
        Ligacao::leitura(),
        "garra_read_history",
        json!({ "chat": "x", "campo_inventado": 1 }),
    )
    .await;
    assert!(
        !resp["error"].is_null(),
        "campo desconhecido passou: {resp}"
    );
}

#[tokio::test]
async fn tool_inexistente_vira_erro_de_protocolo() {
    let resp = chamar(Ligacao::leitura(), "garra_formata_o_disco", json!({})).await;
    assert!(!resp["error"].is_null(), "{resp}");
}

// ── criterio 3: sem aprovacao, nao envia ─────────────────────────────────────

/// O coracao do criterio 3. Na instalacao de leitura, `garra_send_message`:
///
/// - **nao aparece** em `tools/list` (o modelo do outro lado nao planeja em
///   cima de uma capacidade que nao existe); e
/// - **recusa** quando chamada direto, com o motivo estavel `send_disabled`.
///
/// As duas metades importam: "nao anunciada" sozinho seria seguranca por
/// obscuridade — um chamador que leu a doc chamaria pelo nome.
#[tokio::test]
async fn send_message_sem_aprovacao_nao_envia() {
    let l = Ligacao::leitura();
    let nomes = nomes_das_tools(&tools_list(l).await);
    assert!(
        !nomes.iter().any(|n| n == "garra_send_message"),
        "tool de envio anunciada num Garra que recusa todo envio: {nomes:?}"
    );

    let resp = chamar(
        l,
        "garra_send_message",
        json!({ "channel": "telegram", "chat_id": DESTINO, "text": "nao devia sair" }),
    )
    .await;
    let env = envelope(&resp);
    assert_eq!(env["ok"], false, "{env}");
    assert_eq!(env["error"]["kind"], "send_disabled", "{env}");
    assert_eq!(
        resp["result"]["isError"],
        json!(true),
        "a recusa tem de chegar como isError para o host: {resp}"
    );
}

/// O erro de configuracao mais provavel: o interruptor ligado e a allowlist
/// esquecida. **Nao envia**, e o motivo nomeia a allowlist — `allow_send`
/// sozinho nunca foi suficiente.
#[tokio::test]
async fn interruptor_ligado_sem_allowlist_nao_envia() {
    let l = Ligacao::leitura().com(true, false);
    let nomes = nomes_das_tools(&tools_list(l).await);
    assert!(
        !nomes.iter().any(|n| n == "garra_send_message"),
        "{nomes:?}"
    );

    let env = envelope(
        &chamar(
            l,
            "garra_send_message",
            json!({ "channel": "telegram", "chat_id": DESTINO, "text": "nao devia sair" }),
        )
        .await,
    );
    assert_eq!(env["ok"], false, "{env}");
    assert_eq!(env["error"]["kind"], "no_allowlist", "{env}");
}

/// Com tudo liberado, um destino **fora** da allowlist continua recusado. E o
/// que separa "o operador ligou o envio" de "o operador aprovou este destino".
#[tokio::test]
async fn destino_fora_da_allowlist_nao_envia_mesmo_com_tudo_ligado() {
    let env = envelope(
        &chamar(
            Ligacao::leitura().com(true, true),
            "garra_send_message",
            json!({ "channel": "telegram", "chat_id": 999_999, "text": "nao devia sair" }),
        )
        .await,
    );
    assert_eq!(env["ok"], false, "{env}");
    assert_eq!(env["error"]["kind"], "target_not_allowed", "{env}");
}

/// O GREEN do RED→GREEN: com o interruptor ligado e o destino aprovado, a
/// autorizacao **deixa** de ser o motivo da recusa — o que sobra e o canal nao
/// estar registrado neste gateway de teste (`channel_offline`).
///
/// E o mais fundo que se prova sem um bot do Telegram do outro lado, e e o que
/// distingue um gate implementado de um `return Err` universal: se o portao
/// nunca abrisse, este teste veria `send_disabled` aqui.
#[tokio::test]
async fn com_destino_aprovado_o_portao_abre_e_a_recusa_passa_a_ser_entrega() {
    let env = envelope(
        &chamar(
            Ligacao::leitura().com(true, true),
            "garra_send_message",
            json!({ "channel": "telegram", "chat_id": DESTINO, "text": "oi" }),
        )
        .await,
    );
    assert_eq!(env["ok"], false, "{env}");
    assert_eq!(
        env["error"]["kind"], "channel_offline",
        "a autorizacao ainda esta barrando um destino aprovado: {env}"
    );
}

/// Canal sem allowlist de destino nao herda a do Telegram.
#[tokio::test]
async fn canal_sem_allowlist_nao_envia() {
    let env = envelope(
        &chamar(
            Ligacao::leitura().com(true, true),
            "garra_send_message",
            json!({ "channel": "discord", "chat_id": DESTINO, "text": "oi" }),
        )
        .await,
    );
    assert_eq!(env["ok"], false, "{env}");
    // O schema anuncia `enum: ["telegram"]`, e o schema e consultivo em MCP —
    // entao o canal e recusado no servidor, nao no anuncio.
    assert_eq!(env["error"]["kind"], "channel_unsupported", "{env}");
}

/// Nenhuma recusa ecoa o destino. Um `chat_id` recusado ainda e a conversa de
/// uma pessoa real, e a resposta vai para um chamador de fora.
#[tokio::test]
async fn nenhuma_recusa_ecoa_o_destino() {
    for l in [
        Ligacao::leitura(),
        Ligacao::leitura().com(true, false),
        Ligacao::leitura().com(true, true),
    ] {
        let env = envelope(
            &chamar(
                l,
                "garra_send_message",
                json!({ "channel": "telegram", "chat_id": DESTINO, "text": "oi" }),
            )
            .await,
        );
        let texto = env.to_string();
        assert!(
            !texto.contains(&DESTINO.to_string()),
            "a resposta ecoou o destino: {texto}"
        );
    }
}
