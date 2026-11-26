//! O buffer de jitter: os pacotes chegam fora de ordem e com atraso variavel,
//! e o som tem de sair a cada 20 ms. Segura `atraso` quadros antes de tocar,
//! poe na ordem pela sequencia RTP (que da a volta em 65535) e diz quando um
//! quadro se perdeu, para quem toca preencher o buraco.
//!
//! A folga se ajusta sozinha. Com fio, 60 ms bastam; no Wi-Fi o pacote chega
//! 100 ms depois do vizinho (ligacao de 26/11 com o Zoiper: 222 atrasados em
//! 1423, todos virando buraco com a folga fixa). Quando nao ha nada guardado,
//! o quadro esta atrasado, nao perdido: espera um quadro em vez de pular, e a
//! folga cresce. Um pacote que chega depois da hora (o vizinho passou na
//! frente) tambem faz a folga crescer um quadro. Quando sobra folga um segundo
//! inteiro, corta o que sobra.
//!
//! `Variacao` mede o mesmo de fora: a variacao entre chegadas da RFC 3550.

use std::collections::HashMap;

/// Mais que isso a frente do que se espera e o outro lado recomecando a
/// sequencia (ou perda longa): o buffer recomeca do zero.
const SALTO_MAXIMO: i32 = 200;
/// Quadros seguidos sem nada para tocar antes de voltar a encher.
const VAZIO_MAXIMO: u32 = 25;
/// Teto do que fica guardado (2 s de voz).
const GUARDA_MAXIMA: usize = 100;
/// De quantos em quantos quadros (1 s) olha se sobrou folga.
const JANELA: u32 = 50;
/// O que fica de folga no pior momento da janela: 40 ms acima do pior atraso.
const FOLGA_MINIMA: usize = 2;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Contagem {
    pub recebidos: u64,
    pub tocados: u64,
    pub perdidos: u64,
    pub atrasados: u64,
    pub repetidos: u64,
    pub recomecos: u64,
    /// Quadros de silencio esperando um atrasado: cada um aumenta a folga.
    pub esperas: u64,
    /// Quadros jogados fora para diminuir a folga que sobrava.
    pub descartados: u64,
}

#[derive(Debug)]
pub struct Jitter<T> {
    atraso: usize,
    guardados: HashMap<u16, T>,
    proximo: Option<u16>,
    tocando: bool,
    vazios: u32,
    /// Chegou um pacote depois da hora dele: a folga nao deu.
    crescer: bool,
    janela: u32,
    menor_folga: usize,
    contagem: Contagem,
}

/// Distancia de `de` ate `ate` com a volta dos 16 bits: positiva a frente.
fn distancia(de: u16, ate: u16) -> i32 {
    i32::from(ate.wrapping_sub(de).cast_signed())
}

impl<T> Jitter<T> {
    pub fn novo(atraso: usize) -> Self {
        Self {
            atraso: atraso.max(1),
            guardados: HashMap::new(),
            proximo: None,
            tocando: false,
            vazios: 0,
            crescer: false,
            janela: 0,
            menor_folga: usize::MAX,
            contagem: Contagem::default(),
        }
    }

    pub fn contagem(&self) -> Contagem {
        self.contagem
    }

    /// Quadros guardados agora: a folga, ou o atraso que o buffer poe no som.
    pub fn folga(&self) -> usize {
        self.guardados.len()
    }

    pub fn insere(&mut self, sequencia: u16, quadro: T) {
        self.contagem.recebidos += 1;
        if let Some(proximo) = self.proximo {
            let d = distancia(proximo, sequencia);
            if d < 0 {
                self.contagem.atrasados += 1;
                self.crescer = true;
                return;
            }
            if d > SALTO_MAXIMO {
                self.recomeca();
            }
        }
        if self.guardados.contains_key(&sequencia) {
            self.contagem.repetidos += 1;
            return;
        }
        if self.guardados.len() >= GUARDA_MAXIMA {
            self.recomeca();
        }
        self.guardados.insert(sequencia, quadro);
    }

    /// O quadro destes 20 ms. `None` e buraco: ainda enchendo ou perdido.
    pub fn retira(&mut self) -> Option<T> {
        if !self.tocando {
            if self.guardados.len() < self.atraso {
                return None;
            }
            self.tocando = true;
            self.vazios = 0;
            self.crescer = false;
            self.janela = 0;
            self.menor_folga = usize::MAX;
            self.proximo = self.menor_guardado();
        }
        let proximo = self.proximo?;
        if self.guardados.is_empty() || self.crescer {
            // Nada chegou: o quadro esta atrasado (ou o outro lado parou).
            // Espera sem andar a sequencia, e quando ele chegar ainda toca.
            // Com `crescer`, ha quadros, mas a folga ja se mostrou curta.
            self.crescer = false;
            self.contagem.esperas += 1;
            if !self.guardados.is_empty() {
                self.confere_folga();
                return None;
            }
            self.vazios += 1;
            if self.vazios >= VAZIO_MAXIMO {
                // O outro lado parou (espera, mudo com supressao): volta a
                // encher em vez de esperar para sempre.
                self.tocando = false;
                self.proximo = None;
                return None;
            }
            self.confere_folga();
            return None;
        }
        self.vazios = 0;
        self.proximo = Some(proximo.wrapping_add(1));
        let quadro = self.guardados.remove(&proximo);
        if quadro.is_some() {
            self.contagem.tocados += 1;
        } else {
            // Ha quadros depois deste guardados: este se perdeu de verdade.
            self.contagem.perdidos += 1;
        }
        self.confere_folga();
        quadro
    }

    /// Guarda a menor folga da janela; no fim dela, corta metade do que
    /// sobrou acima da `FOLGA_MINIMA` (de vez, sem picotar a fala varias
    /// vezes; depois de uma parada de 300 ms volta ao normal em uns 5 s).
    fn confere_folga(&mut self) {
        self.menor_folga = self.menor_folga.min(self.guardados.len());
        self.janela += 1;
        if self.janela < JANELA {
            return;
        }
        let sobra = self.menor_folga.saturating_sub(FOLGA_MINIMA).div_ceil(2);
        for _ in 0..sobra {
            let Some(proximo) = self.proximo else { break };
            self.proximo = Some(proximo.wrapping_add(1));
            if self.guardados.remove(&proximo).is_some() {
                self.contagem.descartados += 1;
            }
        }
        self.janela = 0;
        self.menor_folga = usize::MAX;
    }

    fn recomeca(&mut self) {
        self.guardados.clear();
        self.proximo = None;
        self.tocando = false;
        self.vazios = 0;
        self.crescer = false;
        self.contagem.recomecos += 1;
    }

    /// O mais antigo guardado, olhando a volta da sequencia.
    fn menor_guardado(&self) -> Option<u16> {
        let primeiro = *self.guardados.keys().next()?;
        self.guardados
            .keys()
            .copied()
            .min_by_key(|s| distancia(primeiro, *s))
    }
}

/// A variacao entre chegadas da RFC 3550 (A.8): quanto o intervalo entre dois
/// pacotes que chegam difere do intervalo entre os carimbos deles, alisado.
/// E o "jitter" que o Zoiper e o Wireshark mostram.
#[derive(Debug, Default, Clone, Copy)]
pub struct Variacao {
    anterior: Option<(u32, u32)>,
    /// A variacao vezes 16, em amostras (o jeito inteiro da RFC).
    vezes_16: u64,
    pior: u64,
}

impl Variacao {
    /// `chegada` e o relogio local na taxa do codec (amostras desde o inicio);
    /// `carimbo` e o do pacote RTP.
    pub fn chegou(&mut self, chegada: u32, carimbo: u32) {
        if let Some((chegada0, carimbo0)) = self.anterior {
            let d = i64::from(chegada.wrapping_sub(chegada0).cast_signed())
                - i64::from(carimbo.wrapping_sub(carimbo0).cast_signed());
            self.vezes_16 = self.vezes_16 + d.unsigned_abs() - ((self.vezes_16 + 8) >> 4);
            self.pior = self.pior.max(self.vezes_16 >> 4);
        }
        self.anterior = Some((chegada, carimbo));
    }

    /// A variacao agora e a maior que houve, em ms.
    pub fn ms(&self, taxa: u32) -> (u64, u64) {
        let taxa = u64::from(taxa.max(1));
        ((self.vezes_16 >> 4) * 1000 / taxa, self.pior * 1000 / taxa)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Gerador fixo para as simulacoes darem sempre o mesmo resultado.
    fn sorteio(semente: &mut u32) -> u32 {
        *semente = semente.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *semente >> 16
    }

    /// Toca a cada 20 ms (a partir de `fase`) o que chegou ate ali.
    /// `chegadas`: (ms, sequencia), em qualquer ordem.
    fn simula(
        j: &mut Jitter<u16>,
        mut chegadas: Vec<(u32, u16)>,
        fase: u32,
        ate_ms: u32,
    ) -> Vec<(Option<u16>, usize)> {
        chegadas.sort_unstable();
        let mut fila = chegadas.into_iter().peekable();
        let mut tocados = Vec::new();
        let mut t = fase;
        while t < ate_ms {
            while let Some(&(ms, seq)) = fila.peek()
                && ms <= t
            {
                j.insere(seq, seq);
                fila.next();
            }
            tocados.push((j.retira(), j.folga()));
            t += 20;
        }
        tocados
    }

    #[test]
    fn rede_limpa_fica_nos_60_ms() {
        let mut j = Jitter::novo(3);
        let chegadas = (0..1500u16).map(|i| (u32::from(i) * 20 + 10, i)).collect();
        let tocados = simula(&mut j, chegadas, 3, 30_000);
        let c = j.contagem();
        assert_eq!((c.perdidos, c.esperas, c.descartados), (0, 0, 0));
        assert!(tocados.iter().skip(5).all(|(_, folga)| *folga <= 3));
    }

    /// Wi-Fi: de vez em quando o radio segura os pacotes ate `segura` ms, e
    /// quem vem atras espera na fila (nao passa na frente).
    fn wifi(segura: u32, semente: u32) -> Vec<(u32, u16)> {
        let mut semente = semente;
        let mut ultima = 0;
        (0..1500u16)
            .map(|i| {
                let envio = u32::from(i) * 20;
                let atraso = if sorteio(&mut semente).is_multiple_of(10) {
                    sorteio(&mut semente) % segura
                } else {
                    sorteio(&mut semente) % 10
                };
                ultima = (envio + 10 + atraso).max(ultima);
                (ultima, i)
            })
            .collect()
    }

    #[test]
    fn wifi_que_segura_ate_150_ms_quase_nao_picota() {
        let mut j = Jitter::novo(3);
        let tocados = simula(&mut j, wifi(150, 7), 3, 30_000);
        // Os primeiros 5 s sao o buffer aprendendo; depois, quase nada.
        let buracos = tocados
            .iter()
            .skip(250)
            .filter(|(q, _)| q.is_none())
            .count();
        assert!(buracos <= 15, "{buracos} buracos depois de aprender");
        assert_eq!(j.contagem().perdidos, 0);
        // E sem guardar atraso a mais: ate 150 ms segurados, folga ate 200 ms.
        let maior = tocados.iter().skip(250).map(|(_, f)| *f).max().unwrap();
        assert!(maior <= 10, "folga de {maior} quadros");
    }

    #[test]
    fn fora_de_ordem_faz_a_folga_crescer() {
        let mut j = Jitter::novo(3);
        let mut semente = 7;
        // Cada pacote com atraso proprio de 10 a 130 ms: muitos passam na
        // frente do anterior.
        let chegadas = (0..1500u16)
            .map(|i| (u32::from(i) * 20 + 10 + sorteio(&mut semente) % 120, i))
            .collect();
        let tocados = simula(&mut j, chegadas, 3, 30_000);
        let buracos = tocados
            .iter()
            .skip(250)
            .filter(|(q, _)| q.is_none())
            .count();
        assert!(buracos <= 10, "{buracos} buracos depois de aprender");
        assert!(j.contagem().esperas > 0, "a folga tinha de crescer");
    }

    #[test]
    fn rajada_de_tres_em_tres_nao_picota_depois_de_aprender() {
        let mut j = Jitter::novo(3);
        // O telefone junta 3 quadros e manda de uma vez a cada 60 ms.
        let chegadas = (0..1500u16)
            .map(|i| ((u32::from(i) / 3) * 60 + 65, i))
            .collect();
        let tocados = simula(&mut j, chegadas, 11, 30_000);
        assert!(tocados.iter().skip(100).all(|(q, _)| q.is_some()));
    }

    #[test]
    fn parada_da_rede_toca_tudo_e_depois_encolhe() {
        let mut j = Jitter::novo(3);
        // De 5,0 a 5,3 s nada chega; aos 5,31 s chega tudo de uma vez.
        let chegadas = (0..1500u16)
            .map(|i| {
                let envio = u32::from(i) * 20;
                let chega = if (5000..5300).contains(&envio) {
                    5310
                } else {
                    envio + 10
                };
                (chega, i)
            })
            .collect();
        let tocados = simula(&mut j, chegadas, 3, 30_000);
        let c = j.contagem();
        assert_eq!(c.perdidos, 0, "nada se perdeu, so atrasou");
        assert!(c.esperas <= 16, "{} esperas", c.esperas);
        // Aos 12 s a folga ja voltou para perto do normal.
        assert!(tocados.iter().skip(600).all(|(_, folga)| *folga <= 4));
    }

    #[test]
    fn espera_o_atrasado_em_vez_de_pular() {
        let mut j = Jitter::novo(1);
        j.insere(1, 'a');
        assert_eq!(j.retira(), Some('a'));
        assert_eq!(j.retira(), None);
        j.insere(2, 'b');
        assert_eq!(j.retira(), Some('b'));
        let c = j.contagem();
        assert_eq!((c.esperas, c.perdidos, c.atrasados), (1, 0, 0));
    }

    #[test]
    fn variacao_da_rfc_3550() {
        let mut v = Variacao::default();
        // Chegadas certinhas a cada 160 amostras: variacao zero.
        for i in 0..10u32 {
            v.chegou(i * 160, 1000 + i * 160);
        }
        assert_eq!(v.ms(8000), (0, 0));
        // Um pacote 80 ms (640 amostras) atrasado: sobe 1/16 disso de uma vez.
        v.chegou(10 * 160 + 640, 1000 + 10 * 160);
        assert_eq!(v.ms(8000), (5, 5));
        // E vai sumindo com as chegadas certinhas de novo.
        for i in 11..200u32 {
            v.chegou(i * 160 + 640, 1000 + i * 160);
        }
        assert_eq!(v.ms(8000), (0, 5));
    }

    fn toca<T>(j: &mut Jitter<T>, n: usize) -> Vec<Option<T>> {
        (0..n).map(|_| j.retira()).collect()
    }

    #[test]
    fn segura_o_atraso_antes_de_tocar() {
        let mut j = Jitter::novo(3);
        j.insere(10, 'a');
        j.insere(11, 'b');
        assert_eq!(j.retira(), None);
        j.insere(12, 'c');
        assert_eq!(toca(&mut j, 3), vec![Some('a'), Some('b'), Some('c')]);
    }

    #[test]
    fn poe_na_ordem() {
        let mut j = Jitter::novo(3);
        j.insere(12, 'c');
        j.insere(10, 'a');
        j.insere(11, 'b');
        assert_eq!(toca(&mut j, 3), vec![Some('a'), Some('b'), Some('c')]);
    }

    #[test]
    fn perda_vira_buraco_e_conta() {
        let mut j = Jitter::novo(2);
        j.insere(1, 'a');
        j.insere(3, 'c');
        assert_eq!(toca(&mut j, 3), vec![Some('a'), None, Some('c')]);
        assert_eq!(j.contagem().perdidos, 1);
    }

    #[test]
    fn atrasado_e_repetido_saem_fora() {
        let mut j = Jitter::novo(1);
        j.insere(5, 'a');
        assert_eq!(j.retira(), Some('a'));
        j.insere(4, 'x');
        j.insere(6, 'b');
        j.insere(6, 'y');
        // O atrasado mostrou que a folga e curta: cresce um quadro.
        assert_eq!(j.retira(), None);
        assert_eq!(j.retira(), Some('b'));
        let c = j.contagem();
        assert_eq!(
            (c.atrasados, c.repetidos, c.tocados, c.esperas),
            (1, 1, 2, 1)
        );
    }

    #[test]
    fn atravessa_a_volta_da_sequencia() {
        let mut j = Jitter::novo(3);
        j.insere(0, 'c');
        j.insere(65534, 'a');
        j.insere(65535, 'b');
        assert_eq!(toca(&mut j, 3), vec![Some('a'), Some('b'), Some('c')]);
    }

    #[test]
    fn salto_grande_recomeca() {
        let mut j = Jitter::novo(1);
        j.insere(100, 'a');
        assert_eq!(j.retira(), Some('a'));
        j.insere(30_000, 'z');
        assert_eq!(j.retira(), Some('z'));
        assert_eq!(j.contagem().recomecos, 1);
    }

    #[test]
    fn silencio_longo_volta_a_encher_sem_contar_perda() {
        let mut j = Jitter::novo(2);
        j.insere(1, 'a');
        j.insere(2, 'b');
        toca(&mut j, 2);
        let buracos = toca(&mut j, usize::try_from(VAZIO_MAXIMO).unwrap());
        assert!(buracos.iter().all(Option::is_none));
        assert_eq!(j.contagem().perdidos, 0);
        j.insere(500, 'c');
        assert_eq!(j.retira(), None); // enchendo de novo
        j.insere(501, 'd');
        assert_eq!(toca(&mut j, 2), vec![Some('c'), Some('d')]);
    }
}
