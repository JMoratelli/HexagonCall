# CI

Três verificações, em `.github/workflows/ci.yml`, em todo push e todo pedido para o `main`:

| Job | O que faz |
|---|---|
| `linux` | `cargo fmt --check`, `clippy -D warnings` e todos os testes |
| `windows` | os mesmos testes no Windows, onde o app também vai rodar |
| `dependencias` | `cargo-deny`: licença compatível com a GPL, avisos de segurança, só crates.io |

Os testes do `motor` sobem dois UAs e um servidor com senha em 127.0.0.1: ligação com áudio nos dois sentidos
(cada lado confere a frequência do tom que ouviu), espera e volta (re-INVITE), recusa, desistência, mensagem,
registro e senha errada. Não dependem de PBX nem de rede.

O teste contra o PBX de verdade fica fora do CI: é a `bancada`, com o PBX no `.env` da raiz, que só existe na máquina
de quem testa (`README.md`).

## Hook de pre-push

`.githooks/pre-push` testa o commit que vai subir num worktree limpo, com o que o job `linux` roda. Recusa o push
se algo falhar, e também se o push reescrever o remoto. Ativar uma vez por clone:

```bash
git config core.hooksPath .githooks
```

Recursos: `taskset` nos 12 primeiros núcleos, `CARGO_BUILD_JOBS=12` e `nice` (a máquina é de quem trabalha nela).
O cache de compilação do hook fica em `~/.cache/hexagon-call-prepush`.

## Pacote

`.github/workflows/pacote.yml`, à mão (Actions → Pacote) ou numa tag `vX.Y.Z`. Compila a janela em release no
Windows e roda `pacote/windows.ps1`: o `HexagonCall.exe`, o instalador `HexagonCallSetup-X.Y.Z.exe` (Inno Setup,
`pacote/instalador.iss`) e o `SHA256SUMS.txt`. À mão, fica só o artefato (14 dias). Na tag, que tem de ser igual à
versão do `Cargo.toml`, publica a release com os três arquivos: é ali que o atualizador do app procura.
