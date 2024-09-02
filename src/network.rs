use crate::{Backend, Command, CommandExecutor, RespDecoder, RespEncoder, RespError, RespFrame};
use anyhow::Result;
use futures::SinkExt;
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, Encoder, Framed};
use tracing::{error, info};

#[derive(Debug)]
struct RespFrameCodec;

#[derive(Debug)]
struct RedisRequest {
    frame: RespFrame,
    backend: Backend,
}

#[derive(Debug)]
struct RedisResponse {
    frame: RespFrame,
}

pub async fn stream_handler(stream: TcpStream, backend: Backend) -> Result<()> {
    let mut framed = Framed::new(stream, RespFrameCodec);
    loop {
        let frame = match framed.next().await {
            Some(Ok(frame)) => frame,
            Some(Err(e)) => {
                error!("Error decoding frame: {:?}", e);
                break Err(e);
            }
            None => {
                info!("Client disconnected");
                break Ok(());
            }
        };

        let request = RedisRequest {
            frame,
            backend: backend.clone(),
        };
        let response = request_handler(request).await?;
        info!("Sending response: {:?}", response.frame);
        framed.send(response.frame).await?;
    }
}

async fn request_handler(request: RedisRequest) -> Result<RedisResponse> {
    let (frame, backend) = (request.frame, request.backend);
    let cmd: Command = frame.try_into()?;
    info!("Executing command: {:?}", cmd);
    let response = cmd.execute(&backend);
    Ok(RedisResponse { frame: response })
}

impl<T> Encoder<T> for RespFrameCodec
where
    T: Into<RespFrame>,
{
    type Error = anyhow::Error;

    fn encode(&mut self, item: T, dst: &mut bytes::BytesMut) -> Result<()> {
        let frame = item.into();
        dst.extend_from_slice(&frame.encode());
        Ok(())
    }
}

impl Decoder for RespFrameCodec {
    type Item = RespFrame;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>> {
        match RespFrame::decode(src) {
            Ok(frame) => Ok(Some(frame)),
            Err(RespError::NotComplete) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Error decoding frame: {:?}", e)),
        }
    }
}
