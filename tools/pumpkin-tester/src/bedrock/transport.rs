//! The client side of Bedrock's `NetherNet` transport, as a 1.26.60 client joins by IP: an SDP
//! offer over HTTP to `/v1/join/{network_id}`, then WebRTC with a reliable and an unreliable data
//! channel. Packets bigger than one data-channel message are split into segments, each led by the
//! number of segments still to come.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::{BufMut, Bytes, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{Notify, mpsc},
    time::timeout,
};
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
        RTCIceGatheringState, RTCPeerConnectionState, RTCSessionDescription,
    },
};

const RELIABLE_CHANNEL: &str = "ReliableDataChannel";
const UNRELIABLE_CHANNEL: &str = "UnreliableDataChannel";
/// Segment size the server also uses (`MAX_FRAGMENT_SIZE` in `nethernet/session.rs`).
const MAX_SEGMENT_SIZE: usize = 10_000;
const SIGNALING_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Signaling answers are SDP, a few KiB; anything past this is not a `NetherNet` server.
const MAX_HTTP_RESPONSE: usize = 1 << 20;

/// An HTTP/1.1 response with the parts the bot looks at.
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// One request on a fresh connection, closed after the answer, like the client's signaling calls.
pub async fn http_request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
) -> Result<HttpResponse, String> {
    let exchange = async {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|err| format!("signaling connect to {addr} failed: {err}"))?;
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/sdp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .await
            .map_err(|err| format!("signaling write failed: {err}"))?;
        stream
            .write_all(body)
            .await
            .map_err(|err| format!("signaling write failed: {err}"))?;
        let mut response = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let read = stream
                .read(&mut buffer)
                .await
                .map_err(|err| format!("signaling read failed: {err}"))?;
            if read == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..read]);
            if response.len() > MAX_HTTP_RESPONSE {
                return Err("signaling response too large".to_string());
            }
        }
        parse_http_response(&response)
    };
    timeout(SIGNALING_TIMEOUT, exchange)
        .await
        .map_err(|_| format!("signaling {method} {path} timed out"))?
}

fn parse_http_response(response: &[u8]) -> Result<HttpResponse, String> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("signaling response has no header end")?;
    let head = std::str::from_utf8(&response[..split])
        .map_err(|_| "signaling response head is not UTF-8")?;
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| format!("bad signaling status line in {head:?}"))?;
    let chunked = head.lines().any(|line| {
        line.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    });
    let raw_body = &response[split + 4..];
    let body = if chunked {
        dechunk(raw_body)?
    } else {
        raw_body.to_vec()
    };
    Ok(HttpResponse { status, body })
}

fn dechunk(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let line_end = data
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or("truncated chunked body")?;
        let size_text = std::str::from_utf8(&data[..line_end]).map_err(|_| "bad chunk size")?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| format!("bad chunk size {size_text:?}"))?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Ok(body);
        }
        if data.len() < size + 2 {
            return Err("truncated chunk".to_string());
        }
        body.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}

struct GatheringHandler {
    gathered: Arc<Notify>,
    failed: Arc<Notify>,
}

#[async_trait]
impl PeerConnectionEventHandler for GatheringHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathered.notify_one();
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if matches!(
            state,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) {
            self.failed.notify_one();
        }
    }
}

/// A connected `NetherNet` session: whole batches in and out, without the `0xfe` marker.
pub struct NetherNetClient {
    peer: Arc<dyn PeerConnection>,
    reliable: Arc<dyn DataChannel>,
    packets: mpsc::UnboundedReceiver<Result<Bytes, String>>,
}

impl NetherNetClient {
    /// Joins the server whose signaling listens on `signaling`, as a direct (by IP) join does.
    pub async fn connect(signaling: SocketAddr, network_id: u64) -> Result<Self, String> {
        let gathered = Arc::new(Notify::new());
        let failed = Arc::new(Notify::new());
        let handler = Arc::new(GatheringHandler {
            gathered: gathered.clone(),
            failed: failed.clone(),
        });
        let peer: Arc<dyn PeerConnection> = Arc::new(
            Box::pin(
                PeerConnectionBuilder::new()
                    .with_configuration(RTCConfigurationBuilder::default().build())
                    .with_handler(handler)
                    .with_udp_addrs(vec![SocketAddr::new(signaling.ip(), 0)])
                    .build(),
            )
            .await
            .map_err(|err| format!("WebRTC peer setup failed: {err}"))?,
        );

        let reliable = peer
            .create_data_channel(
                RELIABLE_CHANNEL,
                Some(RTCDataChannelInit {
                    ordered: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(|err| format!("reliable channel setup failed: {err}"))?;
        let unreliable = peer
            .create_data_channel(
                UNRELIABLE_CHANNEL,
                Some(RTCDataChannelInit {
                    ordered: false,
                    max_retransmits: Some(0),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|err| format!("unreliable channel setup failed: {err}"))?;

        exchange_sdp(peer.as_ref(), &gathered, signaling, network_id).await?;

        let (sender, packets) = mpsc::unbounded_channel();
        let (opened_sender, mut opened) = mpsc::unbounded_channel();
        spawn_channel_reader(
            reliable.clone(),
            true,
            sender.clone(),
            opened_sender.clone(),
        );
        spawn_channel_reader(unreliable, false, sender, opened_sender);

        let both_open = async {
            opened.recv().await;
            opened.recv().await;
        };
        tokio::select! {
            result = timeout(CONNECT_TIMEOUT, both_open) => {
                result.map_err(|_| "data channels did not open in time".to_string())?;
            }
            () = failed.notified() => return Err("WebRTC connection failed".to_string()),
        }

        Ok(Self {
            peer,
            reliable,
            packets,
        })
    }

    /// Sends one batch on the reliable channel, split into segments as the server expects.
    pub async fn send(&self, batch: &[u8]) -> Result<(), String> {
        let segments = batch.len().div_ceil(MAX_SEGMENT_SIZE).max(1);
        let remaining_after_first =
            u8::try_from(segments - 1).map_err(|_| "batch needs more than 256 segments")?;
        for (index, chunk) in batch.chunks(MAX_SEGMENT_SIZE).enumerate() {
            let mut segment = BytesMut::with_capacity(chunk.len() + 1);
            // `index` is below `segments`, which fits a u8 plus one.
            segment.put_u8(remaining_after_first - index as u8);
            segment.extend_from_slice(chunk);
            self.reliable
                .send(segment)
                .await
                .map_err(|err| format!("NetherNet send failed: {err}"))?;
        }
        Ok(())
    }

    /// The next whole batch, or `None` once the session has closed.
    pub async fn recv(&mut self) -> Option<Result<Bytes, String>> {
        self.packets.recv().await
    }

    /// A batch that already arrived, without waiting.
    pub fn try_recv(&mut self) -> Option<Result<Bytes, String>> {
        self.packets.try_recv().ok()
    }

    pub async fn close(&self) {
        let _ = self.peer.close().await;
    }
}

/// Offers the local SDP to the server's signaling endpoint and applies its answer.
async fn exchange_sdp(
    peer: &dyn PeerConnection,
    gathered: &Notify,
    signaling: SocketAddr,
    network_id: u64,
) -> Result<(), String> {
    let offer = peer
        .create_offer(None)
        .await
        .map_err(|err| format!("SDP offer failed: {err}"))?;
    peer.set_local_description(offer)
        .await
        .map_err(|err| format!("setting the SDP offer failed: {err}"))?;
    let _ = timeout(Duration::from_secs(3), gathered.notified()).await;
    let offer = peer
        .local_description()
        .await
        .ok_or("WebRTC produced no local description")?;

    let answer = http_request(
        signaling,
        "POST",
        &format!("/v1/join/{network_id}"),
        offer.sdp.as_bytes(),
    )
    .await?;
    let answer_sdp = String::from_utf8(answer.body).map_err(|_| "SDP answer is not UTF-8")?;
    if answer.status != 200 {
        return Err(format!(
            "signaling refused the offer: HTTP {}: {answer_sdp}",
            answer.status
        ));
    }
    // The server signs its answer with `a=identity:`; WebRTC itself has no use for it.
    let answer_sdp = answer_sdp
        .lines()
        .filter(|line| !line.starts_with("a=identity:"))
        .fold(String::new(), |mut sdp, line| {
            sdp.push_str(line);
            sdp.push_str("\r\n");
            sdp
        });
    let answer = RTCSessionDescription::answer(answer_sdp)
        .map_err(|err| format!("bad SDP answer: {err}"))?;
    peer.set_remote_description(answer)
        .await
        .map_err(|err| format!("setting the SDP answer failed: {err}"))?;
    Ok(())
}

fn spawn_channel_reader(
    channel: Arc<dyn DataChannel>,
    reliable: bool,
    sender: mpsc::UnboundedSender<Result<Bytes, String>>,
    opened: mpsc::UnboundedSender<()>,
) {
    tokio::spawn(async move {
        let mut reassembly = Vec::new();
        let mut expected_remaining: Option<u8> = None;
        while let Some(event) = channel.poll().await {
            match event {
                DataChannelEvent::OnOpen => {
                    let _ = opened.send(());
                }
                DataChannelEvent::OnMessage(message) => {
                    let data: &[u8] = &message.data;
                    let Some((&remaining, payload)) = data.split_first() else {
                        let _ = sender.send(Err("empty NetherNet message".to_string()));
                        break;
                    };
                    if !reliable {
                        let _ = sender.send(Ok(Bytes::copy_from_slice(payload)));
                        continue;
                    }
                    if let Some(expected) = expected_remaining
                        && expected != remaining
                    {
                        let _ = sender.send(Err(format!(
                            "out-of-order NetherNet segment: expected {expected}, got {remaining}"
                        )));
                        break;
                    }
                    reassembly.extend_from_slice(payload);
                    if remaining == 0 {
                        expected_remaining = None;
                        let packet = Bytes::from(std::mem::take(&mut reassembly));
                        if sender.send(Ok(packet)).is_err() {
                            break;
                        }
                    } else {
                        expected_remaining = Some(remaining - 1);
                    }
                }
                DataChannelEvent::OnClose | DataChannelEvent::OnError => break,
                _ => {}
            }
        }
    });
}
