# Hexagon Call

Softphone SIP para o computador, com ligações e mensagens, em Rust. O **M Calls** é o flavor de uso interno
(vermelho); o Hexagon Call é o mesmo programa no verde. Não é produto para vender.

Licença: **GNU GPL 3 ou posterior** (`LICENSE`). Toda dependência tem de ter licença compatível, e o CI confere
(`deny.toml`).

## O que já faz

O núcleo foi testado contra um PBX de verdade (Asterisk 13 com chan_sip), e a janela existe
(`janela/`, Tauri), com o desenho do protótipo (`docs/prototipos/softphone.html`).

| | Situação |
|---|---|
| Registro com senha (digest), renovação e desregistro | ok no PBX |
| Mensagem (SIP MESSAGE), saindo e chegando | ok no PBX |
| Presença dos ramais (SUBSCRIBE/NOTIFY, PIDF do chan_sip), com renovação | ok no PBX |
| Ligação de saída: toque, atendimento, CANCEL, motivo do fim (480, 486…) | ok no PBX, atendida com áudio nos dois sentidos |
| Ligação de entrada: toque (180), atender, recusar (486), quem ligou desistiu | ok no PBX, atendida com áudio nos dois sentidos |
| Renegociação no meio da ligação (re-INVITE, UPDATE) e espera (hold) | ok no PBX (re-INVITE); espera nos testes locais |
| Áudio: G.711 (PCMA e PCMU), RTP, RTP simétrico, jitter que se ajusta à rede (Wi-Fi) | ok no PBX |
| Microfone e alto-falante (cpal: ALSA/PipeWire no Linux, WASAPI no Windows), da placa a 48 kHz para os 8 kHz do telefone | ok no PBX, com headset |
| Sinal de vida (CRLF a cada 20 s) para o firewall não barrar os avisos do PBX | ok |
| Várias contas no mesmo aparelho | na janela: cada conta é um `Ua` com porta própria |
| Janela: cadastro de conta, discar, atender, mudo, espera, mensagens, últimas chamadas, bandeja, moldura própria | em uso no PBX |
| Ligação chegando na notificação do sistema (Atender, Recusar), sons, aviso de microfone sem som | na janela |

Falta: cancelamento de eco (com caixa de som em vez de headset), DTMF, transferência, a notificação de ligação no
canto, abrir com o sistema e o instalador. A lista viva fica em `docs/estado.md`.

## Como está organizado

```
nucleo/   lógica pura, sem rede nem som: contas, G.711, RTP, jitter, SDP, presença,
          o motivo de cada fim de ligação. Testado em qualquer lugar.
som/      microfone e alto-falante: a placa de som vira Fonte e Destino do motor.
motor/    o SIP (rsipstack) e o RTP de verdade. Uma conta, um Ua; quem usa chama os
          métodos (liga, atende, desliga, mensagem, assina_presenca) e escuta os eventos.
bancada/  linha de comando para testar o motor contra um PBX, sem a janela.
janela/   o app (Tauri 2): src/ em Rust, ui/ com a tela em HTML, CSS e JS puros.
docs/     estado do projeto, CI e o protótipo da interface.
```

## Compilar e testar

O compilador é fixo em `rust-toolchain.toml` (1.100.0). Os testes não precisam de PBX: dois UAs conversam em
127.0.0.1 e um servidor falso pede senha como o chan_sip.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Ative o hook uma vez por clone; ele roda fmt, clippy e testes no commit que vai subir:

```bash
git config core.hooksPath .githooks
```

## A janela

```bash
cargo run -p janela            # abre a janela; fechar esconde, o app fica na bandeja
cargo run -p janela -- --bandeja   # so na bandeja (para abrir com o sistema)
```

Na primeira vez, abre no cadastro de conta. Os dados ficam em texto, no estilo do GOnnect, em `~/.config/hexagon-call`
(no Windows, `%APPDATA%\hexagon-call`): `contas.toml`, `preferencias.toml`, `historico.txt` e `conversas.txt`. A senha
vai para o chaveiro do sistema (KWallet ou GNOME Keyring; Gerenciador de Credenciais no Windows) e, numa máquina sem
chaveiro, para `senhas.toml`, que só o próprio usuário lê. "Sair", no menu da bandeja, desregistra as contas.

## Bancada (teste contra o PBX)

O PBX de verdade fica num `.env` na raiz, fora do git: só na máquina de quem testa. Copie o `.env.exemplo`, preencha e
deixe só para você (`chmod 600 .env`: a senha fica nele). O que vier na linha de comando ou no ambiente vale mais que o
`.env`; a senha nunca vai como argumento, porque apareceria no `ps`.

```bash
B="cargo run -q -p bancada --"
$B mensagem "teste"                       # para o SIP_DESTINO (ou --para 1002)
$B presenca --segundos 120
$B liga --toca fala.wav --grava volta.wav --segundos 20
$B liga --microfone --alto-falante        # conversa de verdade, pelo headset
$B escuta --atende --tom 440              # registra e atende quem ligar
$B dispositivos                           # microfones e alto-falantes, com o id
$B nivel                                  # volume do microfone, sem ligar
```

`liga`, `mensagem` e `presenca` não registram a conta. `escuta` registra: o chan_sip guarda um contato por
ramal, então o ramal sai de outro aparelho (um softphone, um telefone IP) enquanto a bancada estiver escutando.

O WAV para tocar precisa ser de 8 kHz, mono, 16 bits:
`ffmpeg -i entrada -ar 8000 -ac 1 -sample_fmt s16 fala.wav`.
