# Diretrizes para trabalhar neste repositório

- Não mencionar assistente de IA (Claude, Anthropic ou outro) em mensagens de commit, descrições de PR, comentários
  ou arquivos do repositório, e não adicionar linha `Co-Authored-By` de assistente: o autor dos commits é só a pessoa
  que os assina (Jurandir Moratelli <jurandirmoratelli@gmail.com>).
- O repositório é **público** (GNU GPL 3 ou posterior). Nada de servidor, IP, ramal, nome de cliente ou senha de
  verdade em arquivo versionado. Os testes e os exemplos usam `pbx.exemplo.com.br`, os IPs de documentação
  (`192.0.2.x`) e os ramais 1001, 1002...
- O PBX de verdade fica só no `.env` da raiz, fora do git, na máquina de quem testa (`.env.exemplo` mostra as
  chaves). A bancada lê dele. Nesse `.env` a senha pode ficar (decisão de 26/11/2026), com `chmod 600`.
- Estrutura (um crate por pasta, detalhes no `README.md`):
  - `nucleo/`: lógica pura, sem rede nem som (contas, G.711, RTP, jitter, SDP, presença, o motivo de cada fim).
  - `motor/`: o SIP (rsipstack) e o RTP. Uma conta, um `Ua`; quem usa chama os métodos e escuta os eventos.
  - `som/`: microfone e alto-falante (cpal), com a troca de taxa entre a placa e os 8 kHz do telefone.
  - `bancada/`: linha de comando para testar o motor contra um PBX de verdade.
  - `janela/`: o app em Tauri 2, no molde da Usina do LinuxMachines: `src/` em Rust e `ui/` com a tela em HTML, CSS
    e JS puros, sem bundler. A tela vai embutida no binário: mudou a `ui/`, recompila.
  - `docs/estado.md`: o estado vivo (decisões, o que foi validado contra o PBX e quando, próximos passos).
    Atualizar ao fim de cada etapa.
- Dados do app em texto, no estilo do GOnnect (`~/.config/hexagon-call`): `contas.toml`, `preferencias.toml`,
  `historico.txt`, `conversas.txt`. A senha vai para o chaveiro do sistema; só numa máquina sem chaveiro ela cai em
  `senhas.toml` (0600).
- Clippy duro (no `Cargo.toml` da raiz): nada de `unwrap`, `expect`, `panic` ou indexação fora dos testes, e toda
  conversão numérica que perde valor escrita de propósito. `unsafe` é proibido.
- Os testes não dependem de PBX, rede nem placa de som (dois UAs e um servidor com senha em 127.0.0.1). O hook de
  pre-push roda fmt, clippy e testes num worktree limpo e recusa reescrever o remoto; ativar uma vez por clone:
  `git config core.hooksPath .githooks`. O CI (`docs/ci.md`) repete no Linux e no Windows e confere as licenças.
- O que muda o comportamento contra o PBX se confere na bancada antes de dar por pronto. Registrar a conta de teste
  tira o ramal do aparelho em que ele estiver (o chan_sip guarda um contato por ramal): combinar antes.
- Compilar e testar usam no máximo ~80% da máquina (`taskset -c 0-11`, `CARGO_BUILD_JOBS=12` e `nice` nesta, a
  `jurandir-pc`).
- Textos para quem usa o app, em português com acento. Comentários e identificadores do código, sem acento.
- Excluir qualquer coisa na tela (conta, conversa) pede confirmação.
- Dependência nova só com licença que pode ir num binário GPL 3 (`deny.toml`).
