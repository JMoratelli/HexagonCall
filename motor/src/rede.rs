//! Enderecos: onde fica o PBX e qual IP nosso ele enxerga.

use crate::erro::Erro;
use std::net::{IpAddr, SocketAddr};

/// O endereco do PBX. IPv4 primeiro: o PBX de hoje so tem IPv4, e o IPv6
/// desta maquina e o do Tailscale.
pub async fn resolve(dominio: &str, porta: u16) -> Result<SocketAddr, Erro> {
    let enderecos: Vec<SocketAddr> = tokio::net::lookup_host((dominio, porta)).await?.collect();
    enderecos
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| enderecos.first())
        .copied()
        .ok_or_else(|| Erro::SemEndereco(dominio.to_string()))
}

/// O IP desta maquina na rota ate `destino`: e o que vai no Contact, no Via e
/// no SDP. Um socket UDP "conectado" nao manda nada, so escolhe a rota.
pub fn ip_local_para(destino: SocketAddr) -> Result<IpAddr, Erro> {
    let qualquer: SocketAddr = if destino.is_ipv4() {
        SocketAddr::from(([0, 0, 0, 0], 0))
    } else {
        SocketAddr::from(([0u16; 8], 0))
    };
    let socket = std::net::UdpSocket::bind(qualquer)?;
    socket.connect(destino)?;
    Ok(socket.local_addr()?.ip())
}

/// Um socket para o RTP de uma ligacao, numa porta livre qualquer.
pub async fn socket_rtp(ip: IpAddr) -> Result<tokio::net::UdpSocket, Erro> {
    Ok(tokio::net::UdpSocket::bind(SocketAddr::new(ip, 0)).await?)
}
