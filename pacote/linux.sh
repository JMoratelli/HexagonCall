#!/usr/bin/env bash
# Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
# Licenca GNU GPL 3 ou posterior.
#
# linux.sh - o pacote do Linux: a AppImage HexagonCall-X.Y.Z-x86_64.AppImage,
# o .zsync dela e o SHA256SUMS.txt que o atualizador do app confere antes de
# trocar o arquivo.
#
#   pacote/linux.sh                 compila e empacota
#   pacote/linux.sh --sem-compilar  so empacota o que ja esta em target
#
# Tudo vai para pacote/saida (fora do git). Precisa de node (o empacotador e o
# CLI do Tauri, numa versao fixa, pelo npx) e do zsyncmake (pacote zsync).
# O CI empacota num Ubuntu 22.04: a AppImage leva as bibliotecas de la e roda
# em sistema com glibc igual ou mais nova (Debian 12, Ubuntu 22.04 em diante).
#
# Padrao de publicacao (o mesmo do Windows): o atualizador acha a AppImage
# pelo nome (HexagonCall-X.Y.Z-x86_64.AppImage) e o hash pela linha do
# SHA256SUMS. A informacao de atualizacao embutida na AppImage
# (gh-releases-zsync) e para quem gerencia AppImages, como o Gear Lever e o
# AppImageLauncher: eles acham a versao nova nas releases do GitHub pelo
# .zsync.
set -euo pipefail

compilar=1
for a in "$@"; do
    case "$a" in
        --sem-compilar) compilar=0 ;;
        *) echo "uso: $0 [--sem-compilar]" >&2; exit 2 ;;
    esac
done

raiz="$(cd "$(dirname "$0")/.." && pwd)"
saida="$raiz/pacote/saida"
# O CLI do Tauri que empacota (so empacota: quem compila e o cargo, como no
# Windows).
cli_tauri="2.12.4"
# A do [workspace.package] do Cargo.toml: a mesma do executavel.
versao="$(sed -n 's/^version = "\(.*\)"/\1/p' "$raiz/Cargo.toml" | head -1)"
[ -n "$versao" ] || { echo "nao achei a versao no Cargo.toml" >&2; exit 1; }
nome="HexagonCall-$versao-x86_64.AppImage"
atualizacao="gh-releases-zsync|JMoratelli|HexagonCall|latest|HexagonCall-*-x86_64.AppImage.zsync"
echo ">> Hexagon Call $versao"

cd "$raiz"
if [ "$compilar" = 1 ]; then
    cargo build --release --locked -p janela
fi
alvo="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
bundle="$alvo/release/bundle/appimage"
rm -rf "$bundle"

# O GStreamer vai junto: os sons da tela (chamando, teclas, fim, mensagem)
# sao WebAudio, que no WebKitGTK toca pelo GStreamer; sem os plugins dentro,
# a AppImage procuraria os do sistema no caminho do Ubuntu e ficaria muda
# em outra distribuicao.
config='{"bundle":{"active":true,"linux":{"appimage":{"bundleMediaFramework":true}}}}'
(
    cd janela
    LDAI_UPDATE_INFORMATION="$atualizacao" APPIMAGE_EXTRACT_AND_RUN=1 \
        npx --yes "@tauri-apps/cli@$cli_tauri" bundle --bundles appimage --config "$config" --ci
)

# O .zsync do empacotador sai na pasta em que ele rodou, com o nome provisorio
# do Tauri, que nao existe na release: fora (o certo e feito abaixo).
rm -f janela/*.AppImage.zsync
gerada="$(find "$bundle" -maxdepth 1 -name '*.AppImage' -print -quit)"
[ -n "$gerada" ] || { echo "o Tauri nao gerou a AppImage em $bundle" >&2; exit 1; }

mkdir -p "$saida"
find "$saida" -maxdepth 1 -type f -delete
cp "$gerada" "$saida/$nome"
chmod 755 "$saida/$nome"

cd "$saida"
# O runtime responde as opcoes --appimage-* sem montar nada (vale sem FUSE).
embutida="$("./$nome" --appimage-updateinformation 2>/dev/null || true)"
if [ "$embutida" != "$atualizacao" ]; then
    echo "a AppImage saiu sem a informacao de atualizacao (veio: '$embutida')" >&2
    exit 1
fi
# O .zsync com o nome final, que e o anexo da release.
zsyncmake -u "$nome" -o "$nome.zsync" "$nome" >/dev/null

# SHA256SUMS.txt no formato do sha256sum ("hash  nome").
sha256sum "$nome" "$nome.zsync" > SHA256SUMS.txt
cat SHA256SUMS.txt
ls -l
