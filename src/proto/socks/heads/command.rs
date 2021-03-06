use std::convert::TryInto;
use futures::Future;
use futures::future::Either;
use failure::Error;
use tokio_io::io::read_exact;
use tokio_io::io::write_all;

use proto::socks::TcpRequestHeader;
use proto::socks::consts;
use proto::socks::SocksError;
use proto::socks::Address;
use std::net::Shutdown;
use tokio::net::TcpStream;
use std::net::SocketAddr;
use bytes::BytesMut;
use bytes::BufMut;
use proto::socks::consts::Reply;
use proto::socks::TcpResponseHeader;
use super::super::codec::write_address;

pub fn read_command(r: TcpStream, peer: SocketAddr)
    -> impl Future<Item = (TcpStream, TcpRequestHeader), Error = Error> + Send
{

    read_exact(r, [0u8; 3]).map_err(|e|-> Error { e.into() })
        .and_then(move |(r, buf)| {
            let ver = buf[0];
            if ver != consts::SOCKS5_VERSION {
                Either::A(
                write_command_response(r, Reply::ConnectionRefused, peer)
                    .and_then(|s| {
                        s.shutdown(Shutdown::Both).map_err(|e| e.into())
                    })
                    .then(move |_r| {
                        Err(SocksError::SocksVersionNoSupport {ver}.into())
                    })
                )
            } else {
                let cmd = buf[1];
                Either::B(match cmd.try_into() {
                    Ok(c) => {
                        Either::A(write_command_response(r, Reply::SUCCEEDED, peer)
                            .map(move |s| (s, c)))
                    }
                    Err(e) => {
                        let e: SocksError = e;
                        Either::B(write_command_response(r, Reply::CommandNotSupported, peer)
                            .and_then(|s| {
                                s.shutdown(Shutdown::Both).map_err(|e| e.into())
                            })
                            .then(|_r| Err(e.into())))
                    }
                })
            }
        })
        .and_then(|(r, command)| {
            Address::read_from(r).map(move |(conn, address)| {
                let header =
                    TcpRequestHeader { command: command,
                        address: address, };

                (conn, header)
            })
        })
}

fn write_command_response(s: TcpStream, rep: Reply, addr: SocketAddr) -> impl Future<Item=TcpStream, Error=Error> {
    let addr = Address::SocketAddress(addr);
    let resp = TcpResponseHeader::new(rep, addr);
    let mut buf = BytesMut::with_capacity(resp.len());
    buf.put_slice(&[consts::SOCKS5_VERSION, resp.reply as u8, 0x00]);
    write_address(&resp.address, &mut buf);
    write_all(s, buf)
        .map(|(s, _b)| s)
        .map_err(|e| e.into())
}