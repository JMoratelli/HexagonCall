//! Hexagon Call - Copyright (c) 2026 Jurandir Moratelli (@JJMoratelli).
//! Licenca GNU GPL 3 ou posterior (LICENSE na raiz do repositorio).
//!
//! O motor: SIP (rsipstack) e RTP de verdade para uma conta. Cada `Ua` e uma
//! conta com o seu endpoint, a sua porta e o seu registro; varias contas no
//! mesmo aparelho sao varios `Ua`. Quem usa (a bancada, depois a janela)
//! chama os metodos e escuta os `Evento`s.

pub mod erro;
pub mod midia;
mod rede;
pub mod ua;

pub use erro::Erro;
pub use ua::{Evento, Opcoes, Registro, Sentido, Sonda, Ua};
