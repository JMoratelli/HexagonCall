# Hexagon Call

Softphone SIP para o computador, com ligações e mensagens, em Rust. O **M Calls** é o flavor de uso interno
(vermelho); o Hexagon Call é o mesmo programa no verde. Não é produto para vender.

Licença: **GNU GPL 3 ou posterior** (`LICENSE`). Toda dependência tem de ter licença compatível, e o CI confere
(`deny.toml`).

## Instalar

Os pacotes ficam nas [releases](https://github.com/JMoratelli/HexagonCall/releases):

- **Windows** (10 ou mais novo): o instalador `HexagonCallSetup-X.Y.Z.exe`, que não pede administrador;
- **Linux** (Debian 12, Ubuntu 22.04 ou mais novos): a `HexagonCall-X.Y.Z-x86_64.AppImage`. Marcar como executável e
  abrir; o atalho no menu o app põe sozinho.

Os dois se atualizam sozinhos pela mesma página, conferindo o sha256 do `SHA256SUMS.txt` da release.

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
| Atendida em outro ramal: o grupo de ramais ou a fila que tocou aqui e outro atendeu não vira perdida (o `Reason` do CANCEL, RFC 3326) | ok no PBX |
| Gravação das ligações em Opus (as duas vozes, uns 5 MB por hora), com retenção e "Ouvir" no histórico | ok no PBX |
| Renegociação no meio da ligação (re-INVITE, UPDATE) e espera (hold) | ok no PBX (re-INVITE); espera nos testes locais |
| Áudio: G.711 (PCMA e PCMU), RTP, RTP simétrico, jitter que se ajusta à rede (Wi-Fi) | ok no PBX |
| Microfone e alto-falante (cpal: ALSA/PipeWire no Linux, WASAPI no Windows), da placa a 48 kHz para os 8 kHz do telefone | ok no PBX, com headset |
| Sinal de vida (CRLF a cada 20 s) para o firewall não barrar os avisos do PBX | ok |
| Excluir do histórico (selecionar vários, com as gravações), nome dado ao ramal, presença em Ramais, aviso de mensagem nova | na janela (o aviso de mensagem só no Linux por ora) |
| Cancelamento de eco (AEC3 do WebRTC em Rust, o `sonora`), redução de ruído do microfone e voz mais nítida (RNNoise) no que chega | nos testes locais, com eco sintético; falta uma ligação de verdade com caixa de som |
| Sonda do PBX (OPTIONS a cada 30 s, como o qualify do Asterisk): o tempo de resposta na tela e, sem resposta, a conta sobe de novo sozinha | nos testes locais e na janela, com um PBX falso que cai e volta |
| Várias contas no mesmo aparelho | na janela: cada conta é um `Ua` com porta própria |
| Janela: cadastro de conta, discar, atender, mudo, espera, mensagens, últimas chamadas, bandeja, moldura própria | em uso no PBX |
| Ligação chegando na notificação do sistema (Atender, Recusar), sons, aviso de microfone sem som | na janela |
| Tons do teclado na ligação (DTMF, RFC 4733) e transferência (REFER) | nos testes locais |
| Toques de entrada (cinco discretos e três fortes, para sala com barulho), tocar em todos os alto-falantes no volume máximo, abrir com o sistema | na janela |
| Instalação em massa: `ocultaConfig = 1` tira da tela as contas e quase todas as opções, que vêm dos arquivos | na janela |
| Perdidas à vista: o ponto vermelho no ícone da bandeja (com quantas na dica), o número na barra de tarefas e o aviso com "Ligar de volta", um por número; zeram quando alguém mexe na tela em Chamadas. No histórico, as perdidas repetidas do mesmo número viram uma linha com o selo de quantas foram | na janela, com um PBX falso; o aviso e o sobreposto do Windows ainda não vistos no Windows |
| Fila, grupo de ramais e siga-me sem "Offline": o PBX manda para a fila o mesmo "Not online" do aparelho desligado, então o número que tocou com a presença Offline deixa de mostrar Offline (`sem-presenca.txt`) | nos testes locais |
| Qualidade da ligação ao vivo: três barras e, quando piora, de quem é o problema (a conexão daqui com o PBX, pela sonda de 2 em 2 s, ou o outro lado, pela perda e a variação do que chega) | nos testes locais; falta calibrar no PBX |
| Efeitos de chamada no padrão da colmeia: o favo em volta de quem liga, as ondas enquanto toca, o favo respirando com a voz do outro lado (e a sua no botão Mudo), o fim e a ligação que caiu | na janela |
| Status no alto da janela: Disponível ou Não perturbe (recusa como ocupado, 486; fica no histórico), a conexão de cada conta e o Reconectar | na janela, com um PBX falso |
| Instalador do Windows e atualização sozinha (releases do GitHub, conferida por sha256) | testado no Windows 10 |
| AppImage do Linux e atualização sozinha (a mesma release, conferida por sha256), atalho no menu | testada no CachyOS (AppImage do Ubuntu 22.04) |

Falta: transferência com consulta, mostrar a outra ligação que chega no meio de uma e a tela em inglês; no Windows, o
mudo do sistema, a pausa da música e o aviso de mensagem nova. A lista viva fica em `docs/estado.md`.

## Como está organizado

```
nucleo/   lógica pura, sem rede nem som: contas, G.711, RTP, jitter, SDP, presença,
          o motivo de cada fim de ligação. Testado em qualquer lugar.
som/      microfone e alto-falante: a placa de som vira Fonte e Destino do motor; a
          gravação das ligações (Opus num arquivo .opus).
motor/    o SIP (rsipstack) e o RTP de verdade. Uma conta, um Ua; quem usa chama os
          métodos (liga, atende, desliga, mensagem, assina_presenca) e escuta os eventos.
bancada/  linha de comando para testar o motor contra um PBX, sem a janela.
janela/   o app (Tauri 2): src/ em Rust, ui/ com a tela em HTML, CSS e JS puros.
pacote/   os scripts dos pacotes: o instalador do Windows (Inno Setup) e a AppImage do Linux.
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

### Instalar em vários computadores

Para quem administra muitos computadores (um callcenter, por exemplo), o `preferencias.toml` tem a chave
`ocultaConfig`, que só muda no arquivo. Com `ocultaConfig = 1`, a tela esconde Contas, Toques e sons e Janela, e em
Áudio mostra só o primeiro bloco (microfone, alto-falante, pausar música). O resto vem dos arquivos, e a tela não muda.
Na pasta do usuário (`~/.config/hexagon-call`; no Windows, `%APPDATA%\hexagon-call`):

- `contas.toml`: um `[[conta]]` por ramal, com `usuario`, `dominio` e, se quiser, `nome` e `porta`. Sem ele, a
  primeira conta se cadastra na tela mesmo com `ocultaConfig = 1`, e a senha vai para o chaveiro; depois dela, Contas
  some;
- `senhas.toml`: a senha de cada conta, em `[senhas]`, como `"1001@pbx.exemplo.com.br" = "..."`, legível só pelo
  usuário. O app procura a senha no chaveiro e, se não achar, neste arquivo;
- `sem-presenca.txt` (opcional): os números de fila ou de grupo de ramais, um por linha, para não aparecerem como
  "Offline" em Ramais. O app acrescenta sozinho o número que tocou com a presença Offline; a fila em que o próprio
  ramal está responde 480 para ele e nunca toca, e essa vale escrever à mão;
- `preferencias.toml`: `ocultaConfig = 1` e o que mais quiser, como `toque = "alerta"`, `toqueNoMaximo = 1` ou
  `gravar = 1` (as opções de ligar e desligar aceitam `0`/`1` e `false`/`true`). Com `ocultaConfig`,
  `abrirComOSistema = 1` liga o abrir com o sistema a cada vez que o app abre, e `0` desliga. `permitirNaoPerturbe = 0`
  tira o "Não perturbe" do botão de status (vale com ou sem `ocultaConfig`).

Todos podem ser gravados em UTF-8 ou em UTF-16 (o `Out-File` do PowerShell). Ao abrir, o app acrescenta no fim do
`preferencias.toml` as chaves que faltam, no valor padrão (`ocultaConfig = 0`), para mostrar o que dá para mudar; só
`microfone`, `altoFalante` e `abrirComOSistema` ficam de fora quando não têm valor. O que já estava escrito fica como
estava, mas mudar o microfone ou o tema pela tela regrava o arquivo sem os comentários. Na trava, só `0` ou `false`
destravam: um valor que não vale trava. E um `preferencias.toml` com erro (um texto sem aspas, por exemplo) deixa a
tela travada, mostra o erro em Áudio e não é regravado até ser corrigido.

No Windows a ligação chegando aparece como toast com o nome e o ícone do app (o do flavor): o app registra o próprio
AppUserModelID no HKCU ao abrir, e o instalador põe o mesmo no atalho do Menu Iniciar. O toast some quando a ligação é
atendida ou acaba. Sem microfone ou sem alto-falante, a ligação segue em silêncio daquele lado, com o aviso na tela.

A tela é só a do app: o menu do navegador (Inspecionar, Recarregar, Imprimir) e os atalhos dele (F5, Ctrl+R, Ctrl+P,
Ctrl+F, o inspetor) ficam desligados; nos campos de texto o menu de copiar e colar continua.

### Gravação

Em Ajustes → Áudio → Gravação: "Gravar as ligações" grava cada ligação atendida, do atendimento ao fim, com as duas vozes
misturadas em Opus (mono, 8 kHz, 12 kbit/s: uns 5 MB por hora) num arquivo `.opus` que toca no navegador, no VLC e no
player do sistema. Os arquivos ficam em `~/.local/share/hexagon-call/gravacoes` (no Windows,
`%LOCALAPPDATA%\hexagon-call\gravacoes`, a pasta local, que um perfil móvel do domínio não leva junto), com o nome
`AAAA-MM-DD_HH-MM-SS_recebida_1002.opus`, e dentro do arquivo o título e a data. O histórico ganha o "Ouvir" na
ligação gravada. "Guardar por" (7, 30 ou 90 dias, 1 ano ou sempre; 90 de padrão) apaga as mais antigas ao abrir e a
cada 6 horas, só os arquivos com esse nome; escolher um prazo que apagaria alguma agora pede confirmação. A
codificação roda numa thread própria: um disco lento não atrasa a ligação. Desinstalar não apaga as gravações.

## Pacotes e atualização

```powershell
pwsh pacote/windows.ps1     # HexagonCall.exe, HexagonCallSetup-X.Y.Z.exe e SHA256SUMS.txt em pacote/saida
```

```bash
pacote/linux.sh             # HexagonCall-X.Y.Z-x86_64.AppImage, o .zsync e SHA256SUMS.txt em pacote/saida
```

No Windows precisa do Inno Setup 6. O instalador não pede administrador (vai para
`%LOCALAPPDATA%\Programs\HexagonCall`). No Linux precisa de node (o CLI do Tauri, que empacota, numa versão fixa pelo
`npx`) e do `zsyncmake`; a AppImage que vale é a do CI, feita no Ubuntu 22.04 (`docs/ci.md`). Uma tag `vX.Y.Z`,
igual à versão do `Cargo.toml`, faz o workflow Pacote publicar a release com os pacotes dos dois sistemas.

O app procura versão nova sozinho (30 s depois de abrir e a cada 6 horas) e também em Ajustes → Sobre. É o motor do
Atalaia Acessos: varre a lista de releases (não a "latest") e fica com a mais nova que tenha o pacote do sistema, baixa,
confere o sha256 pelo `SHA256SUMS.txt` e desregistra as contas antes de sair (com uma ligação em andamento, desiste e
deixa para depois). No Windows roda o instalador em silêncio, que mostra a própria tela de espera e reabre o app; os
dois últimos instaladores ficam no `%TEMP%` para voltar à versão anterior à mão. No Linux a AppImage nova toma o lugar
do arquivo da que está rodando (a pasta dela precisa aceitar escrita) e abre quando a velha sai; a nova e a que ela
substituiu ficam em `~/.cache/hexagon-call`. Rodando de uma AppImage, o app põe o atalho no menu
(`~/.local/share/applications/org.jj.HexagonCall.desktop`), apontando para o arquivo dela. `HEXAGON_CALL_RELEASES` troca
o endereço da lista (teste com um servidor local).

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
