//! A gravacao de uma ligacao: as duas vozes misturadas, em Opus num arquivo
//! `.opus` (Ogg, RFC 7845), mono a 8 kHz. Voz a 12 kbit/s: uns 5 MB por hora,
//! um decimo do WAV. Toca no navegador, no VLC e no player do sistema.
//!
//! O audio chega a cada 20 ms pelo laco do RTP (`motor::midia::Gravador`); a
//! codificacao e o disco ficam numa thread so da gravacao, para um disco lento
//! nunca atrasar a ligacao. Se ela ficar 5 s para tras, os quadros que chegam
//! se perdem, com aviso no log.

use motor::midia::{AMOSTRAS, Gravador, Quadro, TAXA};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use opus_rs::{Application, OpusEncoder, Signal};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::thread::JoinHandle;
use tracing::warn;

const BITS_POR_SEGUNDO: i32 = 12_000;
/// Uma pagina do Ogg por segundo, gravada no disco na hora: se o app cair, a
/// gravacao perde no maximo esse segundo.
const QUADROS_POR_PAGINA: u64 = 50;
/// A folga da fila da thread: 5 s de audio.
const FOLGA: usize = 250;
/// O Ogg Opus conta o tempo a 48 kHz: 960 por quadro de 20 ms.
const POR_QUADRO_48K: u64 = 960;
const SERIAL: u32 = 1;

/// O que vai dentro do arquivo e os players mostram (TITLE e DATE).
#[derive(Debug, Clone, Default)]
pub struct Etiquetas {
    pub titulo: String,
    /// ISO 8601, com o fuso.
    pub data: String,
}

/// O fluxo Opus dentro do Ogg. Separado da thread para os testes.
struct ArquivoOpus<W: Write> {
    ogg: PacketWriter<'static, W>,
    codificador: OpusEncoder,
    pre_skip: u64,
    quadros: u64,
    /// O ultimo pacote fica esperando o proximo: so no fim se sabe qual e o
    /// ultimo, e ele leva a marca de fim do fluxo.
    pendente: Option<Vec<u8>>,
}

impl<W: Write> ArquivoOpus<W> {
    fn novo(saida: W, etiquetas: &Etiquetas) -> Result<Self, String> {
        let taxa = i32::try_from(TAXA).map_err(|e| e.to_string())?;
        let mut codificador = OpusEncoder::new(taxa, 1, Application::Voip)?;
        codificador.bitrate_bps = BITS_POR_SEGUNDO;
        codificador.set_signal(Signal::Voice);
        // O atraso do codificador, contado a 48 kHz (312 a 8 kHz).
        let pre_skip = u64::try_from(codificador.lookahead()).map_err(|e| e.to_string())?
            * u64::from(48_000 / TAXA);
        let mut ogg = PacketWriter::new(saida);
        let erro = |e: std::io::Error| e.to_string();
        ogg.write_packet(cabecalho(pre_skip)?, SERIAL, PacketWriteEndInfo::EndPage, 0)
            .map_err(erro)?;
        ogg.write_packet(
            etiquetas_opus(etiquetas)?,
            SERIAL,
            PacketWriteEndInfo::EndPage,
            0,
        )
        .map_err(erro)?;
        ogg.inner_mut().flush().map_err(erro)?;
        Ok(Self {
            ogg,
            codificador,
            pre_skip,
            quadros: 0,
            pendente: None,
        })
    }

    fn quadro(&mut self, quadro: &Quadro) -> Result<(), String> {
        let mut pacote = [0u8; 1275];
        let n = self.codificador.encode_i16(quadro, AMOSTRAS, &mut pacote)?;
        let novo = pacote.get(..n).ok_or("o Opus devolveu mais que o pacote")?;
        if let Some(anterior) = self.pendente.replace(novo.to_vec()) {
            self.escreve(anterior, false)?;
        }
        Ok(())
    }

    /// Fecha o fluxo e devolve a saida (o arquivo).
    fn fecha(mut self) -> Result<W, String> {
        if let Some(ultimo) = self.pendente.take() {
            self.escreve(ultimo, true)?;
        }
        let mut saida = self.ogg.into_inner();
        saida.flush().map_err(|e| e.to_string())?;
        Ok(saida)
    }

    fn escreve(&mut self, pacote: Vec<u8>, ultimo: bool) -> Result<(), String> {
        self.quadros += 1;
        let fim_de_pagina = self.quadros.is_multiple_of(QUADROS_POR_PAGINA);
        let info = if ultimo {
            PacketWriteEndInfo::EndStream
        } else if fim_de_pagina {
            PacketWriteEndInfo::EndPage
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        let posicao = self.pre_skip + self.quadros * POR_QUADRO_48K;
        let erro = |e: std::io::Error| e.to_string();
        self.ogg
            .write_packet(pacote, SERIAL, info, posicao)
            .map_err(erro)?;
        if fim_de_pagina || ultimo {
            self.ogg.inner_mut().flush().map_err(erro)?;
        }
        Ok(())
    }
}

/// O OpusHead: versao 1, mono, o atraso, a taxa de entrada, sem ganho e o
/// mapeamento 0 (mono ou estereo).
fn cabecalho(pre_skip: u64) -> Result<Vec<u8>, String> {
    let pre_skip = u16::try_from(pre_skip).map_err(|e| e.to_string())?;
    let mut c = b"OpusHead".to_vec();
    c.extend([1, 1]);
    c.extend(pre_skip.to_le_bytes());
    c.extend(TAXA.to_le_bytes());
    c.extend(0i16.to_le_bytes());
    c.push(0);
    Ok(c)
}

/// O OpusTags: quem codificou e os comentarios no formato do Vorbis.
fn etiquetas_opus(e: &Etiquetas) -> Result<Vec<u8>, String> {
    let tamanho = |t: &str| u32::try_from(t.len()).map_err(|e| e.to_string());
    let mut c = b"OpusTags".to_vec();
    let fornecedor = concat!("Hexagon Call ", env!("CARGO_PKG_VERSION"));
    c.extend(tamanho(fornecedor)?.to_le_bytes());
    c.extend(fornecedor.as_bytes());
    let comentarios: Vec<String> = [("TITLE", &e.titulo), ("DATE", &e.data)]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    c.extend(
        u32::try_from(comentarios.len())
            .map_err(|e| e.to_string())?
            .to_le_bytes(),
    );
    for comentario in &comentarios {
        c.extend(tamanho(comentario)?.to_le_bytes());
        c.extend(comentario.as_bytes());
    }
    Ok(c)
}

/// A gravacao de uma ligacao, pronta para o `Sessao::grava` do motor.
pub struct Gravacao {
    fila: Option<SyncSender<Quadro>>,
    thread: Option<JoinHandle<()>>,
    perdidos: u64,
}

impl Gravacao {
    /// Cria o arquivo (nunca por cima de um que ja existe) e a thread.
    pub fn cria(caminho: &Path, etiquetas: &Etiquetas) -> Result<Self, String> {
        let erro = |e: std::io::Error| format!("{}: {e}", caminho.display());
        let arquivo: File = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(caminho)
            .map_err(erro)?;
        let opus = ArquivoOpus::novo(BufWriter::new(arquivo), etiquetas)?;
        let (fila, chegando) = mpsc::sync_channel::<Quadro>(FOLGA);
        let nome = caminho.display().to_string();
        let thread = std::thread::Builder::new()
            .name("gravacao".into())
            .spawn(move || {
                let mut opus = opus;
                for quadro in chegando {
                    if let Err(e) = opus.quadro(&quadro) {
                        warn!(arquivo = %nome, "a gravacao parou: {e}");
                        return;
                    }
                }
                if let Err(e) = opus.fecha() {
                    warn!(arquivo = %nome, "a gravacao nao fechou: {e}");
                }
            })
            .map_err(erro)?;
        Ok(Self {
            fila: Some(fila),
            thread: Some(thread),
            perdidos: 0,
        })
    }

    /// Espera a thread terminar de gravar (depois do `fim`). Para os testes e
    /// para quem precisa do arquivo completo na hora.
    pub fn aguarda(&mut self) {
        self.fim();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Gravador for Gravacao {
    fn quadro(&mut self, nosso: &Quadro, deles: &Quadro) {
        let Some(fila) = &self.fila else { return };
        let mut mistura = [0i16; AMOSTRAS];
        for ((m, a), b) in mistura.iter_mut().zip(nosso).zip(deles) {
            *m = a.saturating_add(*b);
        }
        match fila.try_send(mistura) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.perdidos += 1,
            // A thread parou (erro de disco, ja no log).
            Err(TrySendError::Disconnected(_)) => self.fila = None,
        }
    }

    fn fim(&mut self) {
        // Sem a fila, a thread termina o que tem e fecha o arquivo.
        self.fila = None;
        if self.perdidos > 0 {
            warn!(
                quadros = self.perdidos,
                "a gravacao ficou para tras e perdeu audio"
            );
            self.perdidos = 0;
        }
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod testes {
    use super::*;
    use ogg::reading::PacketReader;
    use opus_rs::OpusDecoder;
    use std::io::Cursor;

    fn tom(frequencia: f32, n: usize) -> Vec<Quadro> {
        let mut fase = 0.0f32;
        let passo = std::f32::consts::TAU * frequencia / 8000.0;
        (0..n)
            .map(|_| {
                let mut q = [0i16; AMOSTRAS];
                for a in &mut q {
                    *a = (fase.sin() * 8000.0) as i16;
                    fase += passo;
                }
                q
            })
            .collect()
    }

    fn cruzamentos(amostras: &[f32]) -> f32 {
        let c = amostras
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        c as f32 / 2.0 / (amostras.len() as f32 / 8000.0)
    }

    #[test]
    fn o_arquivo_tem_cabecalho_etiquetas_e_o_tempo_certo() {
        let etiquetas = Etiquetas {
            titulo: "Ligação recebida de 1002".into(),
            data: "2026-11-27T10:30:00-03:00".into(),
        };
        let mut opus = ArquivoOpus::novo(Vec::new(), &etiquetas).unwrap();
        let quadros = tom(440.0, 120);
        for q in &quadros {
            opus.quadro(q).unwrap();
        }
        let bytes = opus.fecha().unwrap();
        // 2,4 s de voz a 12 kbit/s: perto de 3,6 KB, longe dos 38 KB do WAV.
        assert!(bytes.len() < 8000, "{} bytes", bytes.len());

        let mut leitor = PacketReader::new(Cursor::new(bytes));
        let cab = leitor.read_packet_expected().unwrap();
        assert_eq!(&cab.data[..8], b"OpusHead");
        assert_eq!(cab.data[9], 1, "mono");
        assert_eq!(u16::from_le_bytes([cab.data[10], cab.data[11]]), 312);
        assert_eq!(
            u32::from_le_bytes(cab.data[12..16].try_into().unwrap()),
            8000
        );
        assert!(cab.last_in_page(), "o OpusHead vai sozinho na pagina");
        let tags = leitor.read_packet_expected().unwrap();
        let texto = String::from_utf8_lossy(&tags.data);
        assert!(texto.starts_with("OpusTags"));
        assert!(texto.contains("TITLE=Ligação recebida de 1002"));
        assert!(texto.contains("DATE=2026-11-27T10:30:00-03:00"));

        let mut decodificador = OpusDecoder::new(8000, 1).unwrap();
        let mut som = Vec::new();
        let mut pacotes = 0u64;
        let mut ultimo = None;
        while let Some(p) = leitor.read_packet().unwrap() {
            let mut q = [0f32; AMOSTRAS];
            decodificador.decode(&p.data, AMOSTRAS, &mut q).unwrap();
            som.extend_from_slice(&q);
            pacotes += 1;
            ultimo = Some((p.last_in_stream(), p.absgp_page()));
        }
        assert_eq!(pacotes, 120);
        assert_eq!(ultimo, Some((true, 312 + 120 * 960)));
        let f = cruzamentos(&som[800..]);
        assert!((400.0..480.0).contains(&f), "voltou com {f} Hz");
    }

    #[test]
    fn a_gravacao_mistura_os_dois_lados_e_nao_sobrescreve() {
        let pasta = std::env::temp_dir().join(format!("hexagon-call-grava-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let caminho = pasta.join("ligacao.opus");
        let mut g = Gravacao::cria(&caminho, &Etiquetas::default()).unwrap();
        let (nosso, silencio) = (tom(440.0, 100), [0i16; AMOSTRAS]);
        // Metade da ligacao fala um lado, metade o outro.
        for (i, q) in nosso.iter().enumerate() {
            if i < 50 {
                g.quadro(q, &silencio);
            } else {
                g.quadro(&silencio, q);
            }
        }
        g.aguarda();
        let bytes = std::fs::read(&caminho).unwrap();
        let mut leitor = PacketReader::new(Cursor::new(bytes));
        leitor.read_packet_expected().unwrap();
        leitor.read_packet_expected().unwrap();
        let mut n = 0;
        while leitor.read_packet().unwrap().is_some() {
            n += 1;
        }
        assert_eq!(n, 100);
        assert!(
            Gravacao::cria(&caminho, &Etiquetas::default()).is_err(),
            "nao grava por cima de uma gravacao"
        );
        std::fs::remove_dir_all(&pasta).unwrap();
    }
}
