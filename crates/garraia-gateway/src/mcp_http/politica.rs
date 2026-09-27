//! As decisoes da ponte MCP HTTP, sem relogio, sem I/O e sem `AppState`.
//!
//! Mesmo recorte do `spinner.rs` da CLI e do `state.rs` do
//! `garraia-desktop-core`: o que e *decisao* mora aqui, puro e testavel a seco;
//! o que e *efeito* (ler `AppState`, alcancar um canal, responder JSON-RPC)
//! mora no [`super::handler`]. A separacao existe porque a decisao
//! interessante deste modulo — "este envio sai?" — e exatamente a que ninguem
//! quer ter de provar com um gateway de pe e um bot do Telegram do outro lado.

use garraia_config::AppConfig;

use crate::channel_send::ProactiveTargets;

/// O unico canal que `garra_send_message` alcanca na v1.
///
/// Nao e falta de vontade: a allowlist de destino que serve de aprovacao
/// out-of-band ([`ProactiveTargets`]) so existe para o Telegram
/// (`channels.<canal>.proactive_chat_ids`). Um canal sem allowlist nao tem como
/// dizer "o dono liberou este destino", e sem isso o envio nao teria aprovacao
/// nenhuma — entao ele fica de fora em vez de virar uma excecao.
pub const CANAL_DE_ENVIO: &str = "telegram";

/// A sessao contra a qual o teto de envios da ponte e cobrado.
///
/// Uma so, e nao uma por chamador: o [`crate::channel_send::SendBudget`] e
/// anti-amplificacao, e um orquestrador que pudesse escolher a chave do proprio
/// teto nao teria teto. Todo envio por MCP HTTP divide os mesmos 5 por minuto.
pub const SESSAO_DO_TETO: &str = "mcp-http";

/// A politica viva da ponte, derivada da config a cada chamada.
///
/// Derivada por chamada e nao lida uma vez no boot, pelo mesmo motivo que o
/// `telegram_send` le a allowlist por chamada (auditoria do #921): o operador
/// que tira um destino da lista — ou que desliga `allow_send` — espera que
/// aquilo valha agora, nao no proximo restart. A janela em que um destino
/// revogado ainda funciona e precisamente a coisa errada de se ter.
#[derive(Debug, Clone)]
pub struct PoliticaMcpHttp {
    /// `gateway.mcp_http.allow_send`.
    envio_liberado: bool,
    /// Destinos que o operador liberou, de `proactive_chat_ids`.
    destinos: ProactiveTargets,
    /// Teto de mensagens por chamada de `garra_read_history`.
    teto_do_historico: usize,
}

/// Por que um envio nao saiu.
///
/// Enum e nao `String` porque cada variante e um teste: a mensagem que chega ao
/// chamador pode mudar de redacao sem que a suite pare de provar *qual* trava
/// segurou o envio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recusa {
    /// `gateway.mcp_http.allow_send` esta em `false` (o default).
    InterruptorDesligado,
    /// O interruptor esta ligado, mas nenhum destino foi liberado. O estado de
    /// quem ligou a chave e esqueceu a allowlist — e que nao envia nada.
    SemAllowlist,
    /// Ha allowlist, e este destino nao esta nela.
    ForaDaAllowlist,
    /// Canal diferente de [`CANAL_DE_ENVIO`].
    CanalNaoSuportado,
}

impl Recusa {
    /// O motivo estavel, para o campo `reason` da resposta e para o log.
    ///
    /// Nunca leva o `chat_id`: um destino recusado ainda e a conversa de uma
    /// pessoa real, e ela nao tem por que aparecer no log de um pedido que veio
    /// de fora.
    pub fn codigo(self) -> &'static str {
        match self {
            Self::InterruptorDesligado => "send_disabled",
            Self::SemAllowlist => "no_allowlist",
            Self::ForaDaAllowlist => "target_not_allowed",
            Self::CanalNaoSuportado => "channel_unsupported",
        }
    }

    /// A explicacao que o orquestrador le — com o passo que destrava, quando
    /// existe um. Um "nao" sem o proximo passo faz o chamador tentar de novo.
    pub fn explicacao(self) -> &'static str {
        match self {
            Self::InterruptorDesligado => {
                "envio por MCP esta desligado neste Garra. O operador precisa por \
                 `gateway.mcp_http.allow_send: true` na config e reiniciar. Nao ha \
                 como aprovar isso pela propria ponte."
            }
            Self::SemAllowlist => {
                "nenhum destino foi liberado pelo operador. Ele precisa listar os chats \
                 em `channels.<canal>.proactive_chat_ids` — `allow_send` sozinho nao envia."
            }
            Self::ForaDaAllowlist => {
                "esse destino nao esta em `proactive_chat_ids`. So o operador pode \
                 adiciona-lo, na config."
            }
            Self::CanalNaoSuportado => {
                "so o canal `telegram` tem allowlist de destino neste Garra, entao e o \
                 unico que a ponte MCP alcanca."
            }
        }
    }
}

impl PoliticaMcpHttp {
    /// Le a politica da config viva.
    pub fn da_config(config: &AppConfig) -> Self {
        Self {
            envio_liberado: config.gateway.mcp_http.allow_send,
            destinos: ProactiveTargets::from_config(config),
            teto_do_historico: config.gateway.mcp_http.max_history_messages,
        }
    }

    /// Construtor puro para os testes, que assim nunca montam um `AppConfig`
    /// inteiro so para exercitar uma trava.
    #[cfg(test)]
    pub(crate) fn nova(envio_liberado: bool, destinos: ProactiveTargets) -> Self {
        Self {
            envio_liberado,
            destinos,
            teto_do_historico: 50,
        }
    }

    /// `garra_send_message` esta na superficie anunciada?
    ///
    /// **Uma tool que nao pode enviar nao e anunciada.** Um `tools/list` que
    /// mostra `garra_send_message` num Garra que vai recusar todo envio faz o
    /// modelo do outro lado planejar em cima de uma capacidade que nao existe —
    /// e depois interpretar a recusa como falha transitoria e tentar de novo.
    ///
    /// Note que isto e so o *anuncio*: a recusa em [`Self::decidir_envio`] nao
    /// depende dele. Nao existe "desligada mas chamavel".
    pub fn anuncia_envio(&self) -> bool {
        self.envio_liberado && !self.destinos.is_empty()
    }

    /// Este envio sai?
    ///
    /// Fail-closed em ordem: interruptor, canal, existencia da allowlist,
    /// pertinencia a ela. O primeiro "nao" e o que volta — e de proposito o
    /// mais generico primeiro, para que a resposta nao vire um oraculo de
    /// pertinencia (com o interruptor desligado, todo destino da a mesma
    /// resposta, e o chamador nao descobre quem esta na lista).
    pub fn decidir_envio(&self, canal: &str, chat_id: i64) -> Result<(), Recusa> {
        if !self.envio_liberado {
            return Err(Recusa::InterruptorDesligado);
        }
        if canal != CANAL_DE_ENVIO {
            return Err(Recusa::CanalNaoSuportado);
        }
        if self.destinos.is_empty() {
            return Err(Recusa::SemAllowlist);
        }
        if !self.destinos.allows(chat_id) {
            return Err(Recusa::ForaDaAllowlist);
        }
        Ok(())
    }

    /// Quantas mensagens `garra_read_history` devolve, dado o que o chamador
    /// pediu. Pedido ausente ou acima do teto viram o teto; `0` tambem, porque
    /// um historico vazio por pedido do chamador nao e resposta util.
    pub fn limite_do_historico(&self, pedido: Option<usize>) -> usize {
        let teto = self.teto_do_historico.max(1);
        match pedido {
            Some(n) if n > 0 => n.min(teto),
            _ => teto,
        }
    }

    /// Quantos destinos o operador liberou. So para diagnostico — os ids nunca
    /// saem daqui.
    pub fn destinos_liberados(&self) -> usize {
        self.destinos.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn com_destino(envio: bool, ids: &[i64]) -> PoliticaMcpHttp {
        PoliticaMcpHttp::nova(envio, ProactiveTargets::from_ids(ids.iter().copied()))
    }

    /// O default da instalacao: nada ligado, nada liberado. Nenhum envio sai, e
    /// o motivo e o interruptor — nao a allowlist.
    #[test]
    fn default_recusa_pelo_interruptor() {
        let p = com_destino(false, &[]);
        assert_eq!(
            p.decidir_envio(CANAL_DE_ENVIO, 42),
            Err(Recusa::InterruptorDesligado)
        );
        assert!(!p.anuncia_envio());
    }

    /// O interruptor ligado sem allowlist e o erro de configuracao mais
    /// provavel, e ele **nao** envia. E a metade "aprovacao" do gate: a chave
    /// destrava a tool, a lista aprova o destino.
    #[test]
    fn interruptor_ligado_sem_allowlist_nao_envia() {
        let p = com_destino(true, &[]);
        assert_eq!(
            p.decidir_envio(CANAL_DE_ENVIO, 42),
            Err(Recusa::SemAllowlist)
        );
        assert!(!p.anuncia_envio());
    }

    #[test]
    fn destino_fora_da_allowlist_nao_envia() {
        let p = com_destino(true, &[7]);
        assert_eq!(
            p.decidir_envio(CANAL_DE_ENVIO, 42),
            Err(Recusa::ForaDaAllowlist)
        );
        // Mas a tool existe: ha destino liberado, so nao esse.
        assert!(p.anuncia_envio());
    }

    #[test]
    fn destino_liberado_envia() {
        let p = com_destino(true, &[42]);
        assert_eq!(p.decidir_envio(CANAL_DE_ENVIO, 42), Ok(()));
        assert!(p.anuncia_envio());
    }

    /// Um canal sem allowlist nao herda a do Telegram.
    #[test]
    fn canal_sem_allowlist_e_recusado_mesmo_com_destino_liberado() {
        let p = com_destino(true, &[42]);
        assert_eq!(
            p.decidir_envio("discord", 42),
            Err(Recusa::CanalNaoSuportado)
        );
    }

    /// Com o interruptor desligado, destino liberado e destino desconhecido dao
    /// a MESMA resposta. E o que impede a recusa de virar consulta a allowlist.
    #[test]
    fn interruptor_desligado_nao_distingue_destino_liberado_de_estranho() {
        let p = com_destino(false, &[42]);
        assert_eq!(
            p.decidir_envio(CANAL_DE_ENVIO, 42),
            p.decidir_envio(CANAL_DE_ENVIO, 999)
        );
    }

    #[test]
    fn limite_do_historico_respeita_teto_e_pedido() {
        let p = com_destino(false, &[]);
        assert_eq!(p.limite_do_historico(None), 50, "ausente = teto");
        assert_eq!(p.limite_do_historico(Some(0)), 50, "zero = teto");
        assert_eq!(p.limite_do_historico(Some(10)), 10);
        assert_eq!(
            p.limite_do_historico(Some(5_000)),
            50,
            "acima do teto = teto"
        );
    }

    /// Cada recusa tem codigo proprio: o chamador distingue "o operador nao
    /// ligou" de "este destino nao vale" sem ler prosa.
    #[test]
    fn cada_recusa_tem_codigo_distinto() {
        let codigos: Vec<&str> = [
            Recusa::InterruptorDesligado,
            Recusa::SemAllowlist,
            Recusa::ForaDaAllowlist,
            Recusa::CanalNaoSuportado,
        ]
        .iter()
        .map(|r| r.codigo())
        .collect();
        let unicos: std::collections::BTreeSet<&str> = codigos.iter().copied().collect();
        assert_eq!(unicos.len(), codigos.len(), "{codigos:?}");
    }

    /// A explicacao nunca ecoa um destino. Ela e `&'static str`, entao isto e
    /// uma guarda contra quem trocar o tipo por `String` formatada mais tarde.
    #[test]
    fn explicacao_nao_carrega_destino() {
        for r in [
            Recusa::InterruptorDesligado,
            Recusa::SemAllowlist,
            Recusa::ForaDaAllowlist,
            Recusa::CanalNaoSuportado,
        ] {
            let texto = r.explicacao();
            assert!(!texto.contains("42"), "{texto}");
            assert!(!texto.is_empty());
        }
    }

    /// A politica sai da config de verdade, incluindo o default desligado.
    #[test]
    fn da_config_le_o_default_desligado() {
        let p = PoliticaMcpHttp::da_config(&AppConfig::default());
        assert!(!p.anuncia_envio());
        assert_eq!(p.destinos_liberados(), 0);
        assert_eq!(
            p.decidir_envio(CANAL_DE_ENVIO, 1),
            Err(Recusa::InterruptorDesligado)
        );
    }
}
