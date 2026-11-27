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
  - baixa e confere o sha256 (pelo `SHA256SUMS.txt` da release);
  - instala por cima e reabre o app.
  No Linux, o mesmo caminho com a AppImage: baixar, conferir e trocar o arquivo. A "loja" do Linux é a release do
  GitHub (decisão de 26/11, em vez do Flathub): o app se atualiza sozinho de lá, e a AppImage leva a informação de
  atualização do GitHub (`gh-releases-zsync`) para o Gear Lever e o AppImageLauncher acharem a versão nova. No
  Discover e na Central do GNOME só entraria como Flatpak.
- Base SIP: **rsipstack** (MIT), sem as features de TLS e WebSocket, que o PBX não usa. G.711, RTP, jitter, SDP e
  presença ficam no `nucleo`, nossos e testados.

## Decisões (27/11/2026)

- **AppImageHub por último**, quando o resto estiver pronto. É o catálogo das lojas de AppImage (AppImagePool e outras;
  o Discover e a Central do GNOME não mostram AppImage): um PR em `AppImage/appimage.github.io` com o arquivo
  `data/Hexagon_Call` e a linha `https://github.com/JMoratelli/HexagonCall/`. O teste deles roda a AppImage da release
  mais nova no Ubuntu LTS mais antigo com suporte (a nossa já é feita no 22.04), sem rede, e exige a **interface em
  inglês** por padrão: antes do PR, traduzir a tela (português quando o `LANG` for `pt_*`). Depois de aceito, as
  releases novas entram sozinhas.
- **Atendida em outro ramal**: o grupo de ramais ou a fila que tocou aqui e outro atendeu não é perdida. O Asterisk
  derruba as outras pernas com um CANCEL que leva `Reason: SIP;cause=200;text="Call completed elsewhere"` (ou
  `Q.850;cause=26`); o rsipstack responde o CANCEL sozinho, então o motor lê o `Reason` antes, num `MessageInspector`
  (`motor/src/ua.rs`). No histórico, em âmbar (pedido de 27/11), fora das "Perdidas".
- **Gravação das ligações** (Ajustes → Áudio): Opus num `.opus` (Ogg), as duas vozes misturadas, mono a 8 kHz e
  12 kbit/s (uns 5 MB por hora). O codificador é o `opus-rs` (o libopus 1.6 portado para Rust, BSD-3): sem C para
  compilar no Windows nem na AppImage. Grava do atendimento ao fim, quando a opção está ligada. Retenção de 7, 30 ou
  90 dias, 1 ano ou sempre (90 de padrão), aplicada ao abrir e a cada 6 horas; escolher um prazo que apagaria alguma
  gravação agora pede confirmação. O "Ouvir" no histórico toca dentro do app.
- **Só a interface do app**: o menu do navegador (Inspecionar, Recarregar) e os atalhos dele (F5, Ctrl+R, Ctrl+P,
  Ctrl+F, o inspetor) desligados pela tela, nos dois sistemas; nos campos de texto o menu de copiar e colar fica (no
  Linux; no Windows não, ver abaixo). O WebView2 tem uma opção própria para os atalhos, mas o Tauri não a expõe, e
  chamá-la direto pediria `unsafe`.
- **No Windows, sem o botão direito também nos campos** (pedido dele em 27/11): o menu do WebView2 nos campos de uma
  linha trazia o "Importar Senhas" do Edge, que abria uma janela solta do Edge com erro
  (`edge://settings/autofill/passwords`, ERR_INVALID_URL). Não sai com `autocomplete="off"`, com
  `generalAutofillEnabled: false` nem desligando as features do Edge; o `ContextMenuRequested` do WebView2, que tiraria
  só o item, pede `unsafe`. Copiar e colar seguem pelo teclado (Ctrl+C, Ctrl+V).
- **O offline do próprio ramal faz registrar de novo** (visto num PC com Windows e a 1.0.0, em 27/11): o chan_sip guarda
  um contato por ramal, e outro aparelho do mesmo ramal que sai manda Expires 0, que apaga o registro do ramal inteiro.
  O PBX passava a dar o ramal como offline (480 "Temporarily Unavailable" para quem liga, ligações de saída
  funcionando) com o app verde, que só renovaria aos 450 s; fechar e abrir resolvia. Agora o motor assina a presença
  do próprio ramal depois do primeiro registro (`Ua::assina_propria`, fora da tela) e, quando o PBX a dá como offline
  com a conta registrada, registra de novo na hora (no máximo uma vez a cada 30 s; a renovação a 75% do prazo
  segue). O laço do registro passou a ser nosso: o `keep_alive` do rsipstack não acorda antes da hora. PBX sem hint
  para o ramal recusa o SUBSCRIBE, e fica só a renovação. Na bancada, contra o PBX (27/11, na VM com Windows): a A
  registrada; a B entra no mesmo ramal e sai com Expires 0; o PBX manda o offline e a A registra de novo no mesmo
  segundo; a C liga para o ramal e a A atende, com áudio nos dois sentidos e nenhum pacote perdido. No fim, a A sai
  com um desregistro só. No motor, `o_pbx_da_o_ramal_como_offline_e_a_conta_registra_de_novo`.

### Mais decisões de 27/11

- **Toques fortes** (pedido dele em 27/11, para o callcenter): Recepção (o trinado do telefone de mesa, em duas
  rajadas), Alerta (três bipes por segundo) e Insistente (duas notas alternando, quase sem pausa). Cheios a nota
  inteira e com o timbre áspero de uma onda quadrada (até o 5º harmônico), em vez de subir e sumir como os discretos:
  cada um tem mais que o dobro da força do mais forte dos discretos (`som/src/toque.rs`, teste). Na tela, num grupo à
  parte, "Toques fortes, para sala com barulho".
- **`ocultaConfig`** (pedido dele em 27/11, para instalar em massa): só pelo `preferencias.toml`, `0` de padrão (vale
  também `true`/`false`). Com `1`, somem Contas, Toques e sons e Janela, e Áudio fica só com o primeiro bloco
  (microfone, testar, alto-falante, pausar música). Sem conta, a tela inicial diz "Nenhum ramal neste computador" em
  vez do cadastro. O Rust também recusa pela tela: `salva_conta`, `remove_conta` e ligar ou desligar o abrir com o
  sistema dão erro, e `salva_preferencias` só aceita o que a tela mostra. A senha vem do `senhas.toml`, que o app já
  lia quando o chaveiro não tem a senha. Com `ocultaConfig`, `abrirComOSistema = true/false` no arquivo manda no abrir
  com o sistema a cada abertura. Ao abrir, o app acrescenta no fim do arquivo as chaves que faltam, no padrão, para
  quem administra ver o que existe; um arquivo que não é TOML ou tem seção não se toca.
  Da revisão adversarial (27/11), corrigido antes do commit: completar o arquivo com o `marca` de antes acrescentava o
  `flavor` (chave repetida: tudo voltava ao padrão, destravado) e um `[[seção]]` passava pela guarda; agora o texto
  novo só se grava se valer o mesmo que o velho. Um arquivo que existe e não abre (erro de digitação, UTF-16 do
  `Out-File`) destravava a tela e era regravado com o padrão no primeiro clique: agora o UTF-16 se lê, e o que não
  abre deixa a tela travada, mostra o erro em Áudio e não se grava por cima. Só `0`/`false` destravam (um valor que
  não vale trava). As outras opções de ligar e desligar aceitam `0`/`1` (o `gravar = 1` caía no padrão calado). Com a
  trava, a conta parada se reconecta pelo próprio indicador (a tela Contas some). O completar roda depois da trava de
  instância única. Fica como está: mudar uma opção pela tela regrava o arquivo sem os comentários; com a trava, apagar
  gravações pelo histórico continua liberado (não foi pedido travar).
- **Release 1.0.0** (pedido dele em 27/11): sai com o que está pronto e validado no Linux e no Windows, antes da
  transferência com consulta, da outra ligação no meio de uma e do inglês, que entram nas próximas versões. O
  AppImageHub continua esperando o inglês.
- Na busca do teclado, **um clique no achado já liga**.
- **Estado da conexão claro**: a sonda do PBX (OPTIONS a cada 30 s e, depois de uma sem resposta, de novo em 5 s;
  `motor::Sonda`) diz se o PBX está lá e em quanto tempo responde. Na barra, a conta fica verde só com o nome
  (passando o mouse, "o PBX respondeu em 12 ms, há 8 s"); sem resposta, âmbar com "· sem resposta" ou
  "· reconectando" e "sem resposta desde HH:MM; tenta de novo sozinho a cada 30 s"; vermelho só quando o PBX recusa
  (senha errada). O vigia da rede sobe a conta de novo quando o PBX some com ela registrada (fora de ligação; com
  ligação, depois). O registro que fica sem resposta (o 408 que o rsipstack dá) conta como falta de rede: tenta de
  novo a cada 30 s. Antes, ele ficava em "Falhou" até alguém clicar em Reconectar.
- **Windows por último**: tudo se faz e se valida primeiro no Linux; o Windows vem quando o app do Linux estiver
  pronto (ver "Backlog do Windows").
- **Cancelamento de eco** (pesquisa de 27/11, com bancada sintética): o AEC3 do WebRTC portado para Rust puro
  (`sonora` 0.3.0, BSD-3, sem C), no microfone, com a referência do que o alto-falante tocou de verdade (o callback
  da placa). Na bancada tirou uns 33 dB de eco, como o C++ oficial; com headset fica transparente. **Ligado por
  padrão**, como a supressão de ruído moderada do mesmo WebRTC no microfone. A **voz mais nítida** (o RNNoise,
  `nnnoiseless`, Rust puro) limpa o que chega do outro lado, como ele sugeriu; também ligada por padrão (pedido
  dele em 27/11, depois de usar: "melhora bem a experiência"). O eco só se tira no envio: só ele tem a referência. Tudo junto custa perto de 1% de um núcleo.
  Achado que pesou no desenho: o microfone e a referência perdem amostras juntos, no mesmo ponto; um corte só de um
  lado (a folga do microfone) faz o eco voltar por segundos. Descartados: o `webrtc-audio-processing` (C++), que não
  compila no MSVC e no Ubuntu 22.04 pede meson novo e baixa o abseil; o Speex, mais fraco; o DeepFilterNet3, +31 MB.
- **Próximas, nesta ordem**: mockup da tela (aviso de mensagem nova, presença dos ramais, excluir do histórico com
  seleção de vários, editar o nome na lista de Ramais, a linha "Arquivos" do Sobre); transferência com um botão que
  abre as duas opções, escritas por extenso, "Transferência direta" e "Transferência com consulta" (na com consulta,
  ao desligar, transfere); durante uma ligação, mostrar a outra que chega (hoje é recusada com 486), que usa a mesma
  base de duas ligações ao mesmo tempo da transferência com consulta; e a interface em inglês, escolhida pelo idioma
  do sistema (português em `pt*`).

### O mockup aprovado (27/11), no app

Aprovado por ele ("minimalista, bonitinho") e feito no app no mesmo dia:

- **Excluir do histórico**: a lixeira no cabeçalho de Últimas chamadas entra no modo de seleção (toque nas ligações
  ou "Marcar todas"); "Excluir" pede confirmação, com "Apagar também as gravações delas" marcada. Sai do
  `historico.txt` inteiro (não só das 500 linhas que a tela carrega), pelo começo da ligação, a conta e o ramal.
- **Ramais**: a presença (bolinha no avatar e "Livre", "Em ligação", "Tocando", "Offline"), pela assinatura de
  presença da conta principal aos ramais conhecidos (até 50, os mais recentes; número de fora não), e o **nome dado
  aqui** (o lápis; Enter salva, Esc cancela), em `nomes.toml`, que vale mais que o nome que vem do PBX.
- **Mensagem nova** com a janela escondida ou atrás de outra: a notificação do sistema com quem mandou e o texto;
  "Responder" abre a conversa.
- **Sobre**: os caminhos (Arquivos e Gravações) embaixo, em linha inteira.
- Revisão adversarial da exclusão (27/11), corrigido: a caixa "apagar também as gravações" desmarcada voltava
  marcada num redesenho (uma ligação que acabasse no meio); a ligação era achada só pela hora, a conta e o ramal, e
  pegava duas do mesmo segundo (os dois ligando ao mesmo tempo: a feita, gravada, e a recusada) — agora é a linha
  toda; uma gravação que outra linha ainda usa não se apaga; um `nomes.toml` que não abre não é gravado por cima; e
  regravar um arquivo mantém a permissão e segue o atalho (ele continua atalho).

### Qualidade da voz (27/11), no app

Ajustes → Áudio → Qualidade da voz: "Cancelar o eco", "Reduzir o ruído do microfone" e "Voz mais nítida", as três
ligadas por padrão (pedido de 27/11); valem a partir da próxima ligação. Cada ligação cria a referência (`som::eco::Referencia`): o
alto-falante põe nela o que tocou, o microfone tira o eco com ela (em blocos de 10 ms na taxa da placa, antes de
converter para 8 kHz), e o microfone trocado no meio da ligação pega a mesma quando o velho a solta. Um erro ou
pânico no processador desliga a limpeza naquela ligação, e o som segue. A voz mais nítida só funciona com a saída a
48 kHz (a taxa do RNNoise). Nos testes (`som/src/eco.rs`, sem placa): o eco de uma sala sintética cai mais de 20 dB
depois de 2 s; sem o outro lado, a voz passa (0,3 dB com o eco; cerca de 1 dB com a supressão, depois de ela
aprender o fundo em meio segundo); o ruído constante cai mais de 6 dB.

- [ ] Uma ligação de verdade com caixa de som em vez de headset: o outro lado não deve ouvir o próprio eco.
- [ ] A voz mais nítida numa ligação com ruído do outro lado.

## Backlog do Windows

Só depois que o app do Linux estiver pronto. **Ao fazer e validar cada item no Windows, retirá-lo desta lista.**

- [ ] Mudo do sistema ("Tirar do mudo" no aviso "Não te ouço"): no Windows não faz nada.
- [ ] Pausar música e vídeo ao atender: no Windows não faz nada.
- [ ] Cancelamento de eco, ruído e voz mais nítida numa ligação de verdade (os testes de `som/src/eco.rs` passam no
  Windows, mas nada foi ouvido nele: a VM de teste não tem placa de som).
- [ ] Aviso de mensagem nova como toast (no Linux é a notificação com "Responder"; no Windows, por ora, só o som e o
  número na aba Conversas: `notificacao.rs`, `mensagem`).

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

## Validado no Windows (26/11/2026, Windows 10 22H2 numa VM, contra o PBX)

- [x] fmt, clippy `-D warnings` e todos os testes do workspace no Windows (o que o CI do Windows roda, mais o clippy,
  que pega o código `cfg(windows)`).
- [x] Bancada: mensagem, presença, registro e uma ligação do ramal de teste para ele mesmo pelo PBX (uma bancada
  registrada atende, a outra liga): PCMA, áudio nos dois sentidos, cada lado ouviu o tom do outro (440 e 1000 Hz),
  nenhum pacote perdido.
- [x] O app: a senha lida do Gerenciador de Credenciais, registro, ligação chegando (atendida pela janela e pelo toast,
  recusada pelo toast, perdida), ligação de saída, mensagem de ida e volta (acento e emoji), espera, mudo, tons do
  teclado (os eventos chegam do outro lado pelo Asterisk), instância única, fechar para a bandeja, abrir com o sistema.
- [x] O toast da ligação com o nome e o ícone do app (antes: "Windows PowerShell"), nos dois flavors, e saindo da tela
  quando a ligação é atendida ou acaba (antes ficava empilhado, porque o cenário de ligação não some sozinho).
- [x] Sem microfone e sem alto-falante (a VM não tem placa de som), atender e ligar seguem, com o aviso na tela. Antes o
  atender falhava (só um aviso de 4 s) e quem ligava ficava tocando.
- [x] Transferência sem consulta: o chan_sip manda o NOTIFY com o sipfrag "200 OK" e espera o BYE de quem transferiu
  (RFC 5589), sem derrubar a nossa perna. O motor mandava todo NOTIFY para a presença (481), e a ligação ficava presa
  na tela. Agora o NOTIFY vai para o diálogo da ligação, e com o 2xx final o app desliga: entra como "transferida".
- [x] Instalador (Inno Setup, sem administrador): assistente em português, atalho no Menu Iniciar com o AUMID, a
  conferência do WebView2 (detectou a cópia pela metade da VM: registro dizendo 153, pasta sem o executável) e a
  desinstalação limpa (contas e histórico ficam).
- [x] Atualização de ponta a ponta, com um servidor local no lugar da API do GitHub: a 0.1.0 instalada achou a 0.1.1,
  baixou, conferiu, mostrou a tela de espera do instalador e voltou sozinha na 0.1.1, registrada, em uns 7 s. Recusou
  hash errado (nada instalado, nada sobrou no disco), não ofereceu release sem `SHA256SUMS.txt`, avisou quando o
  servidor falhou, recusou atualizar com ligação em andamento; "Depois" esconde a faixa. O download vai para um nome
  provisório e só vira `HexagonCallSetup-X.Y.Z.exe` depois de conferido (um download ruim não apaga o pacote bom que já
  estava no `%TEMP%`), e "Atualizar agora" ou "Tentar de novo" lê a lista de novo antes de baixar (a guardada podia
  trazer o hash velho de uma release corrigida).
- Na máquina de teste o WebView2 estava quebrado (atualização da Microsoft pela metade); o app abria só com o aviso em
  inglês do Tauri. Fica para o instalador: ele confere e oferece baixar o oficial.

## Validado no Windows (27/11/2026, a mesma VM, contra um PBX falso local)

O que o Linux validou desde 26/11, repetido no Windows com o app em release contra um PBX falso por UDP cru em
127.0.0.1 (registra, responde à sonda, liga com tom de 1 kHz, cancela com ou sem o `Reason`, cala e volta):

- [x] fmt, clippy `-D warnings` e todos os testes do workspace, com os da gravação e do eco.
- [x] Atendida em outro ramal: o toast aparece e sai com o CANCEL; "Ligação de Grupo atendida em outro ramal", a linha
  em âmbar e "Perdidas" sem contar. Sem o `Reason`, perdida.
- [x] Gravação: atendida na janela, o `.opus` em `%LOCALAPPDATA%\hexagon-call\gravacoes`, o "Ouvir" toca dentro do
  WebView2 (0:04 de 0:04; os bytes chegam como `ArrayBuffer`) e o "Abrir" abre a pasta no Explorer. Excluir do
  histórico com "Apagar também a gravação" apaga a linha e o arquivo. A retenção de 7 dias pede o "Apagar" e apaga só
  a vencida; uma com data de antes de 18/11 nunca vence, como manda o código.
- [x] Sonda e reconexão: PBX calado, a barra fica âmbar ("sem resposta", depois "reconectando") uns 24 s depois e o
  REGISTER se repete a cada uns 30 s; o PBX volta e a conta registra em 1 a 2 s, sozinha. Com a porta do PBX fechada
  (o Windows devolve o ICMP como erro 10054 no `recv`), o rsipstack só avisa no log e segue; volta igual.
- [x] Menu e atalhos do navegador: F5, Ctrl+R, Ctrl+Shift+R, Ctrl+P, F, S, O, U, J, H, G, F3, F7, F11, F12,
  Ctrl+Shift+I/J/C, Alt+←, zoom pelo teclado: nada recarrega (o número digitado fica no campo), nada abre. O botão
  direito não abre menu em lugar nenhum, nem nos campos (discar, as buscas, a mensagem), pelo mouse e pelo Shift+F10;
  Ctrl+V cola e Ctrl+C copia no campo.
- [x] Nomes em Ramais (o `nomes.toml`, e o nome novo no histórico) e a mensagem com a janela escondida (sem toast, como
  diz o backlog; o "1" em Conversas).
- [x] Atualização de ponta a ponta com o atualizador novo, com o download lento de propósito: uma ligação que chegou no
  meio fez a troca desistir ("Chegou uma ligação; atualize quando ela acabar"), sem abrir o instalador e com a
  ligação tocando; depois de recusar, "Tentar de novo" instalou a 0.1.1, que voltou sozinha (o Sobre mostra 0.1.1).
- [x] Desinstalar guarda as gravações: sai o ícone das notificações, ficam `gravacoes\` e o `.opus`. Atenção: o Inno
  junta no `unins000.dat` as regras de desinstalação de todas as versões instaladas por cima; uma instalação que já
  teve a regra antiga (apagar a pasta inteira) continua apagando as gravações. Aqui foi só o pacote de teste de 26/11,
  que não saiu em release; uma regra de apagar que for para uma release fica nas instalações para sempre.

## Validado no Linux (26/11/2026, AppImage do Ubuntu 22.04 no CachyOS)

A AppImage feita por `pacote/linux.sh` num contêiner Ubuntu 22.04 (o mesmo sistema do job `linux` do Pacote), rodada
nesta máquina (CachyOS, KDE Wayland, RX 6600), sempre com D-Bus e pastas próprios, sem conta e sem encostar no app
aberto:

- [x] Abre e desenha certo no Wayland nativo, com a GPU e o Mesa daqui, e no Xwayland. Leva o TLS do GIO
  (glib-networking) e o GStreamer com o plugin do PulseAudio (os sons da tela).
- [x] Atalho no menu (`org.jj.HexagonCall.desktop` e o ícone) apontando para o arquivo da AppImage, com
  `StartupWMClass=janela` (o binário dentro dela).
- [x] Atualização de ponta a ponta num KWin virtual, com um servidor local no lugar da API do GitHub: a 0.1.0 achou a
  0.1.1 em 30 s, baixou pelo libsoup, conferiu, trocou o arquivo e a 0.1.1 abriu sozinha em menos de 1 s (o Sobre
  mostra 0.1.1, sem a faixa). A 0.1.0 fica no cache (um link, sem cópia) para voltar à mão. Depois da troca sobra só a
  montagem da nova: a pasta e o fd 1023 que o runtime da AppImage deixa aberto prendiam a velha (e o arquivo velho, já
  apagado) enquanto a nova rodasse. O shell que reabre fecha os dois.
- [x] O mesmo com o app rodando como serviço do systemd (o "abrir com o sistema" do GNOME e do KDE): o serviço acaba
  junto com a velha e levaria o shell que reabre; ele vai num escopo próprio (`systemd-run --scope`) e a 0.1.1 abre.
  No Ubuntu 22.04 (systemd 249) era assim no abrir com o sistema; aqui foi simulado com um serviço `Type=exec`.
- [x] Pasta da AppImage sem escrita: recusa antes de baixar, com o motivo na faixa. Hash errado: baixa, recusa, não
  troca nada e não deixa o provisório no cache.
- Revisão adversarial do atualizador (26/11), corrigido: `APPIMAGE` herdado de outra AppImage (um terminal de editor)
  não vale mais (o binário tem de estar dentro do `APPDIR`); a limpeza do cache não apaga a AppImage que está rodando;
  download comprimido no caminho não vira "parou no meio"; o provisório `.novo` é criado exclusivo (sem seguir link); o
  atalho escapa `%` e reconhece o de outro gerenciador pelo caminho exato; e, nos dois sistemas, uma ligação que chega
  durante o download faz a troca desistir em vez de ser cortada.
- [ ] O som da tela (teclas, chamando) tocando de dentro da AppImage, e uma ligação de verdade por ela.
- [x] A AppImage do CI: o job `linux` do Pacote (run à mão em 26/11) gerou a AppImage, o `.zsync` com o nome final e o
  `SHA256SUMS.txt`, que conferem, com a informação de atualização embutida.
- [x] Uma release de verdade no GitHub: a **1.0.0** (tag `v1.0.0`, 27/11), publicada pelo Pacote com o instalador, o
  `HexagonCall.exe`, a AppImage, o `.zsync` e um `SHA256SUMS.txt` só, que confere baixado da release; a AppImage leva
  a informação de atualização do GitHub.

## Validado em 27/11/2026 (testes locais e a janela num KWin virtual)

- [x] Toques fortes no grupo à parte; `ocultaConfig = 1` com conta (só Áudio, Aparência e Sobre; o "!" do Ajustes
  some, pois quem usa não tem o que fazer) e sem conta ("Nenhum ramal neste computador"); mudar o "Pausar música"
  regrava o arquivo com o `ocultaConfig = 1` e o `abrirComOSistema` de antes; `abrirComOSistema = true` cria a
  entrada do abrir com o sistema e `false` tira. Primeira abertura sem arquivo: o `preferencias.toml` nasce com todas
  as chaves, `ocultaConfig = 0` no fim. Cuidado no teste: o plugin do abrir com o sistema grava em
  `$HOME/.config/autostart`, sem olhar o `XDG_CONFIG_HOME`; a janela de teste precisa de um `HOME` próprio, ou troca a
  entrada de quem usa o computador.

- [x] Atendida em outro ramal: no motor, um PBX falso por UDP cru manda o INVITE e o CANCEL com o `Reason` do chan_sip
  (`atendida_em_outro_ramal_nao_e_perdida`); sem o `Reason` continua perdida. Na janela, o mesmo PBX falso: aviso
  "Ligação de Grupo atendida em outro ramal", a linha no histórico e "Perdidas" sem contar.
- [x] Gravação: o motor entrega os dois lados juntos, a cada 20 ms (`a_gravacao_ouve_os_dois_lados_juntos`); o
  arquivo tem o OpusHead, o título e a data, e o tempo exato no fim (`som/src/gravacao.rs`). Na janela, atendida na
  tela e com 3 s de tom de 1 kHz pelo RTP do PBX falso: "● gravando" na ligação, `.opus` de 3,0 s que o ffmpeg
  decodifica com o tom em 1000 Hz, e o "Ouvir" no histórico tocando dentro do app (no Linux o IPC entrega os bytes como
  lista; no Windows, `ArrayBuffer`: a tela aceita os dois).
- [x] Retenção na tela: escolher 30 dias com uma gravação de 40 pede "Apagar"; Cancelar não apaga nem salva; Apagar
  apaga só ela e salva o prazo.
- Revisão adversarial da retenção (27/11), corrigido: o desinstalador do Windows apagava a pasta local inteira, com as
  gravações (agora só os ícones, e a pasta se ficar vazia); um campo ruim no `preferencias.toml` zerava todas as
  preferências e a retenção caía nos 90 dias do padrão (agora cada campo vale sozinho, e a retenção só apaga com um
  prazo salvo e válido); arquivo com data de antes do app gravar (relógio atrasado) nunca vence; "Ouvir" não segue
  atalho nem abre FIFO; um atalho quebrado ocupa o nome de uma gravação nova; o nome só vale com data e hora possíveis.
- [x] No PBX (27/11, por ele): atendida em outro ramal, com um grupo de ramais com o 4404 e outro aparelho.
- [x] Gravação numa ligação de verdade (27/11, por ele): as duas vozes. O Windows está no backlog do Windows.
- [x] Sonda e reconexão, sintético (27/11): a janela num KWin virtual contra um PBX falso que registra e responde ao
  OPTIONS, e que se cala e volta. Caiu às 08:35:11, a tela mostrou "reconectando" às 08:35:50; voltou às 08:36:36 e
  a conta registrou às 08:36:39, sozinha. No motor, `a_sonda_ve_o_pbx_cair_e_voltar`. A suspensão em si segue pelo
  teste do relógio (`suspensao_e_o_relogio_da_parede_pulando`): ao acordar, a conta sobe de novo pelo mesmo caminho.

## Próximos passos

1. Cancelamento de eco (webrtc-audio-processing, BSD), para quem usa caixa de som. Com headset já dá para usar.
2. Tons do teclado (DTMF, RFC 4733: 5 pacotes de 20 ms + 3 de fim por tecla), espera e transferência sem consulta
   (REFER) conferidos no PBX em 26/11, pelo app no Windows. Transferência com consulta fica para depois.
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
   Ícones (26/11, escolhidos numa prova com cinco opções): o hexágono com sinal, verde no Hexagon Call (H2, o do
   executável: `janela/icons/icon.png` e `.ico`) e vermelho e amarelo no M Calls (M2, `janela/icons/m-calls.*`,
   aplicado na janela e na bandeja quando o flavor é mcalls). Sem a logo do Machadão: o repositório é público.
   Ainda em 26/11: o "Não te ouço" diagnostica (primeiro: a entrada está muda no sistema ou com volume 0%? "Tirar do
   mudo" resolve; só se não estiver, escuta as outras entradas por 1,5 s e sugere a que capta som), "Testar microfone"
   em Ajustes → Áudio (medidor por 10 s), conta principal (`principal = true` no `contas.toml`; a setinha do Ligar
   troca a conta na hora; a conta aparece na ligação que chega e no histórico quando há mais de uma) e pausar música
   e vídeo ao atender (MPRIS no D-Bus; ao desligar, retoma só o que pausou e continua pausado). No Windows, o mudo
   do sistema e o pause ainda não fazem nada.
   Pacote do Windows (26/11): instalador `HexagonCallSetup-X.Y.Z.exe` (`pacote/`), publicado numa tag `vX.Y.Z` pelo
   workflow Pacote, e a atualização pelo motor do Acessos (`janela/src/atualizador.rs`; a rede pela pilha do Windows).
   No Windows o toast da ligação usa o AUMID do app (`janela/src/aumid.rs`).
   Pacote do Linux (26/11): a AppImage `HexagonCall-X.Y.Z-x86_64.AppImage` (`pacote/linux.sh`, empacotada pelo CLI do
   Tauri num Ubuntu 22.04), na mesma release do Windows, com um `SHA256SUMS.txt` só para os dois. A atualização é o
   mesmo motor (rede pelo libsoup do WebKitGTK): a nova toma o lugar do arquivo da que está rodando e abre quando a
   velha sai; a nova e a anterior ficam em `~/.cache/hexagon-call`. Rodando de uma AppImage, o app põe o próprio atalho
   no menu (`janela/src/atalho.rs`), a menos que o Gear Lever ou o AppImageLauncher já tenham posto o deles.
