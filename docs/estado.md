# Estado

O que está validado, o que falta e as decisões tomadas. Atualizar ao fim de cada etapa.

## Decisões (26/11/2026)

- Só **computador**, sem app de celular. Linux e Windows.
- **GNU GPL 3 ou posterior**. Não é para vender.
- Mensagens pelo próprio SIP (MESSAGE). As limitações são conhecidas: o PBX aceita (202), não confirma entrega nem
  leitura, e não guarda para quem está offline. No máximo um aviso de "offline".
- **Janela pequena** (uns 780 × 600), não tela cheia. Abre em Chamadas, no teclado, com o campo em foco e as últimas
  chamadas ao lado. Abas: Chamadas, Conversas, Ramais; configurações na engrenagem.
- **Ligação chegando**: uma notificação pequena no canto da tela, que não se desliga. Trazer a janela principal para a
  frente é opcional nas configurações.
- **Toques discretos**, para não irritar.
- Fechar a janela leva para a **bandeja** (o app continua recebendo ligação).
- **Várias contas** no mesmo aparelho (alguns usam mais de uma). No motor, cada conta é um `Ua`, com porta e registro
  próprios.
- **Distribuição e atualização** (decisão de 26/11): no Linux, **AppImage** publicada nas releases do GitHub, o que
  simplifica a atualização. No Windows, o comportamento de atualização do Atalaia Acessos, código nosso
  (`JMoratelli/acessos`, `internal/atualizador`):
  - varre a **lista** de releases, e não a marcada como "latest", e fica com a mais nova que tenha pacote para a
    plataforma;
  - baixa e confere o sha256;
  - instala por cima e reabre o app.
  No Linux, o mesmo caminho com a AppImage: baixar, conferir e trocar o arquivo.
- Base SIP: **rsipstack** (MIT), sem as features de TLS e WebSocket, que o PBX não usa. G.711, RTP, jitter, SDP e
  presença ficam no `nucleo`, nossos e testados.

## O PBX de teste

Asterisk 13 com chan_sip, UDP 5060, sem TLS, realm `asterisk`. O servidor, os ramais e a senha ficam no `.env` da raiz,
fora do git (só na máquina de quem testa). Medido na bancada:

- Aceita MESSAGE fora de chamada (401 e depois 202) e entrega no outro aparelho com acento e emoji.
- Presença por SUBSCRIBE `Event: presence` (PIDF): `open`/`Ready` para livre. No chan_sip, "ocupado" vem como
  `closed` + `On the phone`, então `closed` sozinho não quer dizer offline (`nucleo/src/presenca.rs`).
- O firewall da máquina de teste (ufw) esquece uma conversa UDP em 120 s sem tráfego e barra o NOTIFY que chega depois. Por
  isso o sinal de vida a cada 20 s.
- Fechar o Zoiper não tira o registro: o "offline" de quem só some depende do PBX perceber.
- Logo depois do ACK de uma ligação que chega, o Asterisk manda um re-INVITE com o mesmo SDP e a versão do `o=`
  somada. Responder 488 faz ele desligar na hora (BYE com `X-Asterisk-HangupCause: Bearer capability not available`,
  causa 58). O motor aceita (200 com o mesmo `o=`), e a ligação segue até alguém desligar (causa 16).
- O outro ramal de teste é um Zoiper no celular, pelo Wi-Fi. Na primeira ligação atendida, com a folga fixa de 60 ms no jitter, 222 dos 1423
  pacotes da volta chegaram depois da hora: buracos de 20 a 40 ms, uns 8 por segundo. Com o jitter que se ajusta
  (`nucleo/src/jitter.rs`), na segunda: 966 de 966 tocados, nenhum perdido ou atrasado, 10 esperas (no começo e
  quando ele desligou), variação entre chegadas de 17 ms (pior 22 ms).

## Validado contra o PBX (26/11/2026, do ramal de teste para o Zoiper)

- [x] MESSAGE (aceita, e entregue no outro aparelho)
- [x] Presença do outro ramal (Livre) e cancelamento da assinatura no fim
- [x] Ligação de saída: INVITE autenticado, 180 em 2 s, CANCEL depois de 45 s sem atender
- [x] Registro (600 s), renovação sozinha a 75% do prazo (450 s) e desregistro no fim
- [x] Ligação de saída atendida em PCMA, com áudio nos dois sentidos: a fala chegou no Zoiper, e a volta, gravada e
  transcrita, chegou inteira
- [x] Conversa pelo microfone e alto-falante do PC (headset HyperX, 48 kHz pelo PipeWire): a volta chegou
  inteira e foi transcrita
- [x] Ligação de entrada (do Zoiper para o ramal de teste) atendida pelo motor, com áudio nos dois sentidos (893 de 894 tocados, nenhum
  perdido; a fala dele chegou e foi transcrita)

## Próximos passos

1. Cancelamento de eco (webrtc-audio-processing, BSD), para quem usa caixa de som. Com headset já dá para usar.
2. Tons do teclado (DTMF, RFC 4733: 5 pacotes de 20 ms + 3 de fim por tecla) e transferência sem consulta (REFER;
   o BYE do PBX entra como "transferida") estão no motor e na janela, com teste local. Falta conferir no PBX, junto
   com a espera (com música de espera ou sem). Transferência com consulta fica para depois.
3. Puxar os ramais do Asterisk com o nome cadastrado: pelo AMI só de leitura, se o fornecedor do PBX liberar.
4. A janela em **Tauri** (decisão de 26/11; no molde da Usina, do LinuxMachines: HTML puro, sem bundler). **Visível
   desde 26/11** (`janela/`): cadastro de conta, discar, atender, recusar, mudo, espera, mensagens, últimas chamadas,
   bandeja, instância única, cores M Calls e Hexagon Call. Os dados ficam em texto, no estilo do GOnnect
   (`~/.config/hexagon-call`: `contas.toml`, `preferencias.toml`, `historico.txt`, `conversas.txt`); a senha no
   chaveiro, ou em `senhas.toml` (0600) numa máquina sem chaveiro. Usada no dia a dia desde 26/11 (ligações de
   verdade, de saída e de entrada). Também em 26/11, a pedido dele: moldura própria (a barra do app é a da janela),
   ligação chegando na notificação do sistema (Atender e Recusar; no KDE, pelo D-Bus), sons sintetizados (Modernos ou
   Clássicos: chamando, teclas, fim, mensagem; o toque de entrada ele aprovou), aviso âmbar "Não te ouço" com troca
   do microfone no meio da ligação (8 s de silêncio absoluto), e a espera sem tocar o outro lado (o Asterisk segue
   mandando o áudio). Ainda em 26/11: cinco toques de entrada tocados pelo Rust (`som::toque`), com volume, e a opção
   de tocar em todos os alto-falantes no volume máximo (no Linux, sobe cada saída a 100% e devolve depois; gravado no
   `preferencias.toml`); abrir com o sistema, só na bandeja. No Linux o som vai pelo protocolo do PulseAudio (o
   PipeWire responde), com buffer de 20 ms; o ALSA cru fica de reserva. Ainda em 26/11: configurações com menu lateral
   (Contas, Áudio, Toques e sons, Aparência, Janela, Sobre) e as opções em grupos; "tocar em todos" e "volume máximo"
   separados; tema Sistema/Claro/Escuro; busca em Ramais. O **flavor** é só pelo arquivo: `flavor = "mcalls"` no
   `preferencias.toml` liga o M Calls; sem ele, Hexagon Call (um binário só; `marca`, o nome de antes, ainda vale).
   **Volta sozinha** (26/11): um vigia a cada 5 s sobe a conta de novo quando o computador volta da suspensão (o
   relógio do motor para enquanto ele dorme, e o registro vencia no PBX), quando o IP para chegar ao PBX muda ou a
   rede cai e volta, e a cada 30 s quando a conta nem subiu ou o PBX não respondeu (o app aberto antes da rede, no
   boot). Recusa de senha não se repete sozinha (o fail2ban do PBX bloquearia o IP). A conta velha sai sem
   desregistrar (`Ua::abandona`): no chan_sip, um Expires 0 apagaria também o registro novo. Com ligação em
   andamento, depois que ela acabar. "Reconectar" na conta faz o mesmo à mão. Falta conferir de verdade: suspender
   o computador e trocar de rede com o app aberto.
   Falta: os ícones dos dois flavors e o pacote (AppImage, instalador do Windows) com a atualização.
