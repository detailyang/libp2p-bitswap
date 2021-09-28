use crate::error::BitswapError;
/// Reperesents a prototype for an upgrade to handle the bitswap protocol.
///
/// The protocol works the following way:
///
/// - TODO
use crate::message::BitswapMessage;
use core::future::Future;
use core::iter;
use core::pin::Pin;
use futures::io::{AsyncRead, AsyncWrite};
use libp2p::core::{upgrade, InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use std::io;
use std::marker::PhantomData;
use tiny_multihash::MultihashDigest;

// Undocumented, but according to JS we our messages have a max size of 512*1024
// https://github.com/ipfs/js-ipfs-bitswap/blob/d8f80408aadab94c962f6b88f343eb9f39fa0fcc/src/decision-engine/index.js#L16
const MAX_BUF_SIZE: usize = 524_288;

#[derive(Clone, Debug)]
pub struct BitswapConfig<MH> {
    _marker: PhantomData<MH>,
}

impl<MH> Default for BitswapConfig<MH> {
    fn default() -> Self {
        Self {
            _marker: Default::default(),
        }
    }
}

impl<MH: MultihashDigest> UpgradeInfo for BitswapConfig<MH> {
    type Info = &'static [u8];
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(b"/chain/ipfs/bitswap/1.1.0")
    }
}

impl<MH, TSocket> InboundUpgrade<TSocket> for BitswapConfig<MH>
where
    MH: MultihashDigest,
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = BitswapMessage<MH>;
    type Error = BitswapError;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    #[inline]
    fn upgrade_inbound(self, mut socket: TSocket, info: Self::Info) -> Self::Future {
        Box::pin(async move {
            log::debug!("upgrade_inbound: {}", std::str::from_utf8(info).unwrap());
            let packet = upgrade::read_length_prefixed(&mut socket, MAX_BUF_SIZE).await?;
            let message = BitswapMessage::from_bytes(&packet)?;
            log::debug!("inbound message: {:?}", message);
            Ok(message)
        })
    }
}

impl<MH: MultihashDigest> UpgradeInfo for BitswapMessage<MH> {
    type Info = &'static [u8];
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(b"/chain/ipfs/bitswap/1.1.0")
    }
}

impl<MH, TSocket> OutboundUpgrade<TSocket> for BitswapMessage<MH>
where
    MH: MultihashDigest,
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = ();
    type Error = io::Error;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    #[inline]
    fn upgrade_outbound(self, mut socket: TSocket, info: Self::Info) -> Self::Future {
        Box::pin(async move {
            log::debug!("upgrade_outbound: {}", std::str::from_utf8(info).unwrap());
            let bytes = self.to_bytes();
            upgrade::write_length_prefixed(&mut socket, bytes).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::net::{TcpListener, TcpStream};
    use futures::prelude::*;
    use libp2p::core::upgrade;
    use tiny_multihash::Multihash;

    #[async_std::test]
    async fn test_upgrade() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();

        let server = async move {
            let incoming = listener.incoming().into_future().await.0.unwrap().unwrap();
            upgrade::apply_inbound(incoming, BitswapConfig::<Multihash>::default())
                .await
                .unwrap();
        };

        let client = async move {
            let stream = TcpStream::connect(&listener_addr).await.unwrap();
            upgrade::apply_outbound(
                stream,
                BitswapMessage::<Multihash>::new(),
                upgrade::Version::V1,
            )
            .await
            .unwrap();
        };

        future::select(Box::pin(server), Box::pin(client)).await;
    }
}
