use thiserror::Error;

#[derive(Debug, Error)]
pub enum Erro {
    // Os dois da biblioteca vao numa caixa: tem mais de 100 bytes e inflariam
    // todo Result do motor.
    #[error("SIP: {0}")]
    Sip(Box<rsipstack::Error>),
    #[error("mensagem SIP invalida: {0}")]
    Mensagem(Box<rsipstack::sip::Error>),
    #[error("rede: {0}")]
    Rede(#[from] std::io::Error),
    #[error(transparent)]
    Conta(#[from] nucleo::conta::ErroConta),
    #[error(transparent)]
    Sdp(#[from] nucleo::sdp::ErroSdp),
    #[error("o PBX respondeu {codigo} {motivo}")]
    Resposta { codigo: u16, motivo: String },
    #[error("o PBX nao respondeu")]
    SemResposta,
    #[error("nao achei o endereco de {0}")]
    SemEndereco(String),
    #[error("a ligacao {0} nao existe (ou ja acabou)")]
    SemLigacao(String),
    #[error("a ligacao {0} nao esta no estado certo para isso")]
    EstadoErrado(String),
    #[error("audio: {0}")]
    Audio(String),
}

impl From<rsipstack::Error> for Erro {
    fn from(e: rsipstack::Error) -> Self {
        Self::Sip(Box::new(e))
    }
}

impl From<rsipstack::sip::Error> for Erro {
    fn from(e: rsipstack::sip::Error) -> Self {
        Self::Mensagem(Box::new(e))
    }
}
