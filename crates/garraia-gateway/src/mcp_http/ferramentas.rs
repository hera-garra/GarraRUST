//! Os descritores das cinco tools e a validacao dos argumentos delas.
//!
//! Puro: nenhuma funcao daqui toca `AppState`, rede ou disco. O que os
//! descritores anunciam e o que [`super::handler`] de fato aceita tem de ser a
//! mesma coisa, e a forma de garantir isso sem um servidor de pe e manter as
//! duas metades no mesmo arquivo — o schema logo acima do `struct` que o
//! desserializa.
//!
//! ## `deny_unknown_fields` nao e decoracao
//!
//! Todo schema aqui leva `additionalProperties: false` e todo `struct` leva
//! `#[serde(deny_unknown_fields)]`. Em MCP o schema e **consultivo**: o host
//! pode mandar o que quiser, e um campo a mais que o servidor ignora em
//! silencio e um pedido que o chamador acha que fez e o servidor acha que
//! nao. Num `garra_send_message` isso seria uma mensagem indo para um destino
//! diferente do pedido.

use std::sync::Arc;

use rmcp::model::{Tool, ToolAnnotations};
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value as JsonValue, json};

use super::politica::{CANAL_DE_ENVIO, PoliticaMcpHttp};

/// Teto do texto de um envio, em caracteres.
///
/// Mesmo valor do `telegram_send` (`tools/channel_send_tool.rs`): o Telegram
/// corta em 4096 unidades UTF-16, e recusar aqui com um motivo legivel e melhor
/// que uma rejeicao da Bot API que o chamador nao sabe interpretar.
pub const MAX_TEXTO_CHARS: usize = 4000;

/// Nomes das cinco tools, em um lugar so.
///
/// Usados pelo descritor, pelo despacho e pelos testes. O despacho e um `match`
/// sobre estas constantes justamente para que renomear uma tool sem atualizar o
/// anuncio nao compile.
pub const TOOL_STATUS: &str = "garra_status";
pub const TOOL_LIST_CHATS: &str = "garra_list_chats";
pub const TOOL_READ_HISTORY: &str = "garra_read_history";
pub const TOOL_SEND_MESSAGE: &str = "garra_send_message";
pub const TOOL_PAIR_STATUS: &str = "garra_pair_status";

/// `serde_json::Value::Object` garantido pelos literais abaixo.
fn objeto(v: JsonValue) -> Arc<JsonMap<String, JsonValue>> {
    match v {
        JsonValue::Object(map) => Arc::new(map),
        _ => unreachable!("todo schema deste modulo e um literal de objeto"),
    }
}

/// Schema de uma tool que nao recebe argumento nenhum.
fn sem_argumentos() -> Arc<JsonMap<String, JsonValue>> {
    objeto(json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    }))
}

/// As anotacoes das quatro tools de leitura.
///
/// `read_only_hint` e um **hint** na spec do MCP — o host nao e obrigado a
/// respeita-lo, e por isso ele nao substitui trava nenhuma deste modulo. Ele
/// serve para o outro lado: um orquestrador que sabe distinguir leitura de
/// escrita pode chamar as quatro sem cerimonia e pedir confirmacao so na
/// quinta, que e exatamente o comportamento que se quer induzir.
fn somente_leitura() -> ToolAnnotations {
    ToolAnnotations::new().read_only(true)
}

fn tool_status() -> Tool {
    Tool::new(
        TOOL_STATUS,
        "Saude deste Garra: versao, uptime, canais e contagem de conversas em memoria. \
         Somente leitura.",
        sem_argumentos(),
    )
    .with_annotations(somente_leitura())
}

fn tool_list_chats() -> Tool {
    Tool::new(
        TOOL_LIST_CHATS,
        "Lista as conversas que este Garra conhece agora, com canal, tamanho do historico \
         e inatividade. Somente leitura. Use o campo `chat` de uma linha como argumento de \
         garra_read_history.",
        objeto(json!({
            "type": "object",
            "properties": {
                "channel": {
                    "type": "string",
                    "description": "Filtra por canal (`telegram`, `web`, `mobile`, ...). \
                                    Omita para listar todos.",
                    "minLength": 1,
                    "maxLength": 64
                }
            },
            "additionalProperties": false
        })),
    )
    .with_annotations(somente_leitura())
}

fn tool_read_history() -> Tool {
    Tool::new(
        TOOL_READ_HISTORY,
        "Ultimas mensagens de uma conversa, com segredos redigidos. Somente leitura. \
         O `chat` e o identificador que garra_list_chats devolve.",
        objeto(json!({
            "type": "object",
            "properties": {
                "chat": {
                    "type": "string",
                    "description": "Identificador da conversa, vindo de garra_list_chats.",
                    "minLength": 1,
                    "maxLength": 256
                },
                "limit": {
                    "type": "integer",
                    "description": "Quantas mensagens do fim da conversa. Acima do teto do \
                                    operador, vale o teto.",
                    "minimum": 1
                }
            },
            "required": ["chat"],
            "additionalProperties": false
        })),
    )
    .with_annotations(somente_leitura())
}

fn tool_send_message() -> Tool {
    Tool::new(
        TOOL_SEND_MESSAGE,
        "Envia uma mensagem num canal real, em nome do operador. Só alcança destinos que o \
         operador liberou na config — não há como aprovar um destino novo por aqui.",
        objeto(json!({
            "type": "object",
            "properties": {
                "channel": {
                    "type": "string",
                    "enum": [CANAL_DE_ENVIO],
                    "description": "Canal de saida. Hoje so `telegram`."
                },
                "chat_id": {
                    "type": "integer",
                    "description": "Chat de destino. Precisa estar em \
                                    `channels.<canal>.proactive_chat_ids`."
                },
                "text": {
                    "type": "string",
                    "description": "Texto da mensagem.",
                    "minLength": 1,
                    "maxLength": MAX_TEXTO_CHARS
                }
            },
            "required": ["channel", "chat_id", "text"],
            "additionalProperties": false
        })),
    )
    // Nao e leitura, nao e idempotente (duas chamadas iguais = duas mensagens
    // na conversa de alguem) e alcanca o mundo de fora. As tres coisas ditas em
    // voz alta, para o host do outro lado tratar esta tool diferente das quatro.
    .with_annotations(ToolAnnotations::from_raw(
        None,
        Some(false),
        Some(false),
        Some(false),
        Some(true),
    ))
}

fn tool_pair_status() -> Tool {
    Tool::new(
        TOOL_PAIR_STATUS,
        "Estado de pareamento dos canais: quais estao ligados, quais esperam segredo e \
         quais estao no ar. Somente leitura.",
        sem_argumentos(),
    )
    .with_annotations(somente_leitura())
}

/// A superficie anunciada em `tools/list`.
///
/// Quatro tools sempre; `garra_send_message` entra somente quando a politica
/// diz que um envio poderia sair (ver
/// [`PoliticaMcpHttp::anuncia_envio`](super::politica::PoliticaMcpHttp::anuncia_envio)).
/// Puro de proposito: um teste fixa a superficie sem subir servidor.
pub fn tools_anunciadas(politica: &PoliticaMcpHttp) -> Vec<Tool> {
    let mut tools = vec![
        tool_status(),
        tool_list_chats(),
        tool_read_history(),
        tool_pair_status(),
    ];
    if politica.anuncia_envio() {
        tools.push(tool_send_message());
    }
    tools
}

/// Argumentos de `garra_list_chats`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgsListChats {
    #[serde(default)]
    pub channel: Option<String>,
}

/// Argumentos de `garra_read_history`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgsReadHistory {
    pub chat: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Argumentos de `garra_send_message`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgsSendMessage {
    pub channel: String,
    pub chat_id: i64,
    pub text: String,
}

/// Validacao de limites de `garra_read_history`. O `Err` e a mensagem que o
/// chamador recebe.
pub fn validar_read_history(args: &ArgsReadHistory) -> Result<(), String> {
    if args.chat.trim().is_empty() {
        return Err("`chat` esta vazio".to_string());
    }
    Ok(())
}

/// Validacao de limites de `garra_send_message`.
///
/// So forma e tamanho — **nao** autorizacao. Quem decide se o envio sai e
/// [`PoliticaMcpHttp::decidir_envio`](super::politica::PoliticaMcpHttp::decidir_envio),
/// e essa ordem importa: validar depois de decidir transformaria a mensagem de
/// erro num oraculo ("esse destino nao vale" versus "esse texto e longo demais"
/// contam coisas diferentes sobre a allowlist).
pub fn validar_send_message(args: &ArgsSendMessage) -> Result<(), String> {
    if args.text.trim().is_empty() {
        return Err("`text` esta vazio".to_string());
    }
    let chars = args.text.chars().count();
    if chars > MAX_TEXTO_CHARS {
        return Err(format!(
            "mensagem muito longa: {chars} caracteres (limite: {MAX_TEXTO_CHARS})"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_send::ProactiveTargets;

    fn politica(envio: bool, ids: &[i64]) -> PoliticaMcpHttp {
        PoliticaMcpHttp::nova(envio, ProactiveTargets::from_ids(ids.iter().copied()))
    }

    /// Os nomes anunciados, na ordem em que o `tools/list` os devolve.
    fn nomes(envio: bool, ids: &[i64]) -> Vec<String> {
        tools_anunciadas(&politica(envio, ids))
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// No default da instalacao a superficie tem as quatro de leitura, e
    /// `garra_send_message` nao aparece.
    #[test]
    fn superficie_default_e_so_leitura() {
        assert_eq!(
            nomes(false, &[]),
            vec![
                TOOL_STATUS,
                TOOL_LIST_CHATS,
                TOOL_READ_HISTORY,
                TOOL_PAIR_STATUS
            ]
        );
    }

    /// Com o operador tendo ligado a chave E liberado um destino, as cinco da
    /// spec aparecem.
    #[test]
    fn superficie_liberada_tem_as_cinco() {
        let nomes = nomes(true, &[42]);
        assert_eq!(nomes.len(), 5, "{nomes:?}");
        assert!(nomes.iter().any(|n| n == TOOL_SEND_MESSAGE));
    }

    /// Interruptor ligado mas allowlist vazia: nada pode sair, e a tool
    /// tampouco e oferecida.
    #[test]
    fn sem_allowlist_a_tool_de_envio_nao_e_anunciada() {
        let nomes = nomes(true, &[]);
        assert!(!nomes.iter().any(|n| n == TOOL_SEND_MESSAGE), "{nomes:?}");
    }

    /// Todo schema anunciado fecha `additionalProperties`. Sem isso, o
    /// `deny_unknown_fields` do lado Rust recusaria um pedido que o schema
    /// dizia ser valido — divergencia entre anuncio e aceitacao.
    #[test]
    fn todo_schema_fecha_additional_properties() {
        for t in tools_anunciadas(&politica(true, &[1])) {
            let schema = JsonValue::Object((*t.input_schema).clone());
            assert_eq!(
                schema["additionalProperties"],
                json!(false),
                "{}: {schema}",
                t.name
            );
            assert_eq!(schema["type"], json!("object"), "{}", t.name);
        }
    }

    /// Toda tool tem descricao nao vazia: e o unico texto pelo qual o modelo do
    /// outro lado decide se chama ou nao.
    #[test]
    fn toda_tool_tem_descricao() {
        for t in tools_anunciadas(&politica(true, &[1])) {
            let d = t.description.as_deref().unwrap_or_default();
            assert!(!d.trim().is_empty(), "{} sem descricao", t.name);
        }
    }

    /// O enum de `channel` do schema nomeia o mesmo canal que a politica
    /// aceita. Divergirem seria anunciar um canal que todo envio recusa.
    #[test]
    fn schema_de_envio_anuncia_o_canal_que_a_politica_aceita() {
        let t = tool_send_message();
        let schema = JsonValue::Object((*t.input_schema).clone());
        assert_eq!(
            schema["properties"]["channel"]["enum"],
            json!([CANAL_DE_ENVIO])
        );
    }

    #[test]
    fn campo_desconhecido_e_recusado() {
        let erro = serde_json::from_value::<ArgsSendMessage>(json!({
            "channel": "telegram",
            "chat_id": 1,
            "text": "oi",
            "chat_id_de_verdade": 2
        }));
        assert!(erro.is_err(), "campo extra passou");
    }

    #[test]
    fn texto_vazio_e_longo_demais_sao_recusados() {
        let base = |texto: String| ArgsSendMessage {
            channel: CANAL_DE_ENVIO.to_string(),
            chat_id: 1,
            text: texto,
        };
        assert!(validar_send_message(&base("   ".into())).is_err());
        assert!(validar_send_message(&base("a".repeat(MAX_TEXTO_CHARS + 1))).is_err());
        assert!(validar_send_message(&base("a".repeat(MAX_TEXTO_CHARS))).is_ok());
    }

    /// Contado em `chars` e nao em bytes: um texto de emoji no limite passa.
    #[test]
    fn limite_de_texto_e_por_caractere_nao_por_byte() {
        let args = ArgsSendMessage {
            channel: CANAL_DE_ENVIO.to_string(),
            chat_id: 1,
            text: "🦀".repeat(MAX_TEXTO_CHARS),
        };
        assert!(validar_send_message(&args).is_ok());
    }

    #[test]
    fn chat_vazio_no_historico_e_recusado() {
        let args = ArgsReadHistory {
            chat: "  ".into(),
            limit: None,
        };
        assert!(validar_read_history(&args).is_err());
    }
}
