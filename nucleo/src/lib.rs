//! Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
//! Licenca GNU GPL 3 ou posterior (LICENSE na raiz do repositorio).
//!
//! O nucleo: tudo o que o softphone decide sem tocar em rede, som ou disco.
//! O motor faz o IO e pergunta aqui; os testes daqui rodam em qualquer lugar.

pub mod conta;
pub mod dtmf;
pub mod g711;
pub mod jitter;
pub mod ligacao;
pub mod presenca;
pub mod qualidade;
pub mod rtp;
pub mod sdp;
