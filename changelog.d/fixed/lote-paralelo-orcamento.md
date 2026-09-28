- **Lote paralelo de ferramentas deixa de furar o orcamento e de derrubar o turno.**
  O modelo pode pedir varias ferramentas numa resposta so, e o orcamento so era
  conferido antes de cada chamada ao modelo: um lote de 15 rodava inteiro contra um
  teto de 10. No modo `search` (piso do WhatsApp pessoal, 10 chamadas por tarefa) o
  turno ainda caia em `execution budget exceeded` depois de tudo executado, sem o
  modelo ver os resultados, e o canal respondia "Tente de novo em instantes" para um
  pedido que falharia de novo. Agora cada chamada confere o orcamento antes de rodar:
  a que passaria do teto nao executa e volta ao modelo dizendo por que. Com o
  orcamento esgotado, o modelo ganha uma volta final, avisada, para responder com o
  que ja coletou, e nada que ele pedir nela roda.
