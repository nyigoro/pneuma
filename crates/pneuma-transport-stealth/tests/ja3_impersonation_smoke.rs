use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use pneuma_engines::{TransportProvider, TransportStealthProfile};
use pneuma_transport_stealth::LocalProxyTransportProvider;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_rustls::TlsConnector;

struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &str, value: String) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var(&self.key, value);
        } else {
            std::env::remove_var(&self.key);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_proxy_endpoint_emits_parseable_ja3_clienthello() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (echo_addr, ja3_rx, echo_task) = spawn_ja3_echo_server().await?;
    let (proxy_addr, proxy_task) = spawn_connect_proxy(echo_addr).await?;

    let _proxy_guard = EnvVarGuard::set(
        "PNEUMA_TRANSPORT_PROXY_CHROME120",
        format!("http://{proxy_addr}"),
    );

    let provider = LocalProxyTransportProvider::new();
    let proxy_cfg = provider
        .proxy_for_profile(&TransportStealthProfile::Chrome120)
        .context("provider should return proxy config for Chrome120")?;

    open_tls_via_proxy(&proxy_cfg.http_proxy, echo_addr).await?;

    let ja3 = tokio::time::timeout(Duration::from_secs(5), ja3_rx)
        .await
        .context("timed out waiting for JA3 capture from echo server")?
        .context("JA3 echo server dropped capture channel")?;

    assert!(
        !ja3.is_empty(),
        "captured JA3 string should not be empty when TLS client hello is observed"
    );

    if let Ok(expected_ja3) = std::env::var("PNEUMA_EXPECTED_JA3_CHROME120") {
        assert_eq!(
            ja3, expected_ja3,
            "captured JA3 does not match expected value from PNEUMA_EXPECTED_JA3_CHROME120"
        );
    }

    proxy_task.abort();
    echo_task.abort();
    Ok(())
}

async fn spawn_ja3_echo_server() -> Result<(
    SocketAddr,
    oneshot::Receiver<String>,
    tokio::task::JoinHandle<()>,
)> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind JA3 echo listener")?;
    let addr = listener.local_addr()?;
    let (tx, rx) = oneshot::channel::<String>();

    let task = tokio::spawn(async move {
        let mut ja3_out = String::new();
        if let Ok((mut stream, _)) = listener.accept().await {
            if let Ok(payload) = read_first_tls_record_payload(&mut stream).await {
                if let Some(ja3) = parse_client_hello_ja3(&payload) {
                    ja3_out = ja3;
                }
            }
        }
        let _ = tx.send(ja3_out);
    });

    Ok((addr, rx, task))
}

async fn spawn_connect_proxy(
    target_addr: SocketAddr,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind CONNECT proxy listener")?;
    let addr = listener.local_addr()?;

    let task = tokio::spawn(async move {
        let Ok((mut downstream, _)) = listener.accept().await else {
            return;
        };

        let Ok(request) = read_http_headers(&mut downstream).await else {
            let _ = downstream.shutdown().await;
            return;
        };

        let first_line = request.lines().next().unwrap_or_default();
        if !first_line.starts_with("CONNECT ") {
            let _ = downstream
                .write_all(
                    b"HTTP/1.1 405 Method Not Allowed\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                )
                .await;
            let _ = downstream.shutdown().await;
            return;
        }

        let Ok(mut upstream) = TcpStream::connect(target_addr).await else {
            let _ = downstream
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\nContent-Length: 0\r\n\r\n")
                .await;
            let _ = downstream.shutdown().await;
            return;
        };

        let _ = downstream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await;
        let _ = tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
    });

    Ok((addr, task))
}

async fn open_tls_via_proxy(proxy_authority: &str, target_addr: SocketAddr) -> Result<()> {
    let mut stream = TcpStream::connect(proxy_authority)
        .await
        .with_context(|| format!("failed to connect to proxy endpoint {proxy_authority}"))?;

    let connect_request = format!(
        "CONNECT {target_addr} HTTP/1.1\r\nHost: {target_addr}\r\nProxy-Connection: Keep-Alive\r\n\r\n"
    );
    stream
        .write_all(connect_request.as_bytes())
        .await
        .context("failed to write CONNECT request to proxy")?;

    let response = read_http_headers(&mut stream)
        .await
        .context("failed to read CONNECT response from proxy")?;
    let status_line = response.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") && !status_line.ends_with(" 200") {
        bail!("proxy CONNECT failed: {status_line}");
    }

    let root_store = rustls::RootCertStore::empty();
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client_cfg));
    let server_name = rustls::pki_types::ServerName::try_from("localhost")
        .context("invalid rustls server name")?;

    // Handshake is expected to fail because the test JA3 echo server does not
    // complete TLS negotiation, but rustls will emit ClientHello first.
    let _ = connector.connect(server_name, stream).await;
    Ok(())
}

async fn read_first_tls_record_payload(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut header = [0_u8; 5];
    stream
        .read_exact(&mut header)
        .await
        .context("failed to read TLS record header")?;
    if header[0] != 22 {
        bail!("first TLS record was not handshake content type");
    }
    let payload_len = u16::from_be_bytes([header[3], header[4]]) as usize;
    let mut payload = vec![0_u8; payload_len];
    stream
        .read_exact(&mut payload)
        .await
        .context("failed to read TLS record payload")?;
    Ok(payload)
}

async fn read_http_headers(stream: &mut TcpStream) -> Result<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0_u8; 1];
    while buf.len() < 16 * 1024 {
        let n = stream
            .read(&mut byte)
            .await
            .context("failed while reading HTTP headers")?;
        if n == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8(buf).context("HTTP header bytes were not UTF-8")
}

fn parse_client_hello_ja3(record_payload: &[u8]) -> Option<String> {
    if record_payload.len() < 4 || record_payload[0] != 1 {
        return None;
    }
    let hello_len = u24_to_usize(&record_payload[1..4]);
    if record_payload.len() < 4 + hello_len {
        return None;
    }

    let hello = &record_payload[4..4 + hello_len];
    let mut cursor = 0_usize;

    let version = read_u16(hello, &mut cursor)?;
    cursor = cursor.checked_add(32)?; // random
    if cursor > hello.len() {
        return None;
    }

    let session_id_len = read_u8(hello, &mut cursor)? as usize;
    cursor = cursor.checked_add(session_id_len)?;
    if cursor > hello.len() {
        return None;
    }

    let cipher_len = read_u16(hello, &mut cursor)? as usize;
    if cursor.checked_add(cipher_len)? > hello.len() || cipher_len % 2 != 0 {
        return None;
    }
    let mut ciphers = Vec::<u16>::new();
    for chunk in hello[cursor..cursor + cipher_len].chunks_exact(2) {
        let val = u16::from_be_bytes([chunk[0], chunk[1]]);
        if !is_grease_u16(val) {
            ciphers.push(val);
        }
    }
    cursor += cipher_len;

    let compression_len = read_u8(hello, &mut cursor)? as usize;
    cursor = cursor.checked_add(compression_len)?;
    if cursor > hello.len() {
        return None;
    }

    let mut extensions = Vec::<u16>::new();
    let mut elliptic_curves = Vec::<u16>::new();
    let mut ec_point_formats = Vec::<u8>::new();

    if cursor < hello.len() {
        let ext_total_len = read_u16(hello, &mut cursor)? as usize;
        if cursor.checked_add(ext_total_len)? > hello.len() {
            return None;
        }
        let mut ext_cursor = cursor;
        let ext_end = cursor + ext_total_len;
        while ext_cursor + 4 <= ext_end {
            let ext_type = u16::from_be_bytes([hello[ext_cursor], hello[ext_cursor + 1]]);
            let ext_len =
                u16::from_be_bytes([hello[ext_cursor + 2], hello[ext_cursor + 3]]) as usize;
            ext_cursor += 4;
            if ext_cursor + ext_len > ext_end {
                return None;
            }

            let ext_data = &hello[ext_cursor..ext_cursor + ext_len];
            if !is_grease_u16(ext_type) {
                extensions.push(ext_type);
            }

            if ext_type == 10 {
                parse_supported_groups(ext_data, &mut elliptic_curves);
            } else if ext_type == 11 {
                parse_ec_point_formats(ext_data, &mut ec_point_formats);
            }

            ext_cursor += ext_len;
        }
    }

    Some(format!(
        "{version},{},{},{},{}",
        join_u16(&ciphers),
        join_u16(&extensions),
        join_u16(&elliptic_curves),
        join_u8(&ec_point_formats),
    ))
}

fn parse_supported_groups(data: &[u8], out: &mut Vec<u16>) {
    if data.len() < 2 {
        return;
    }
    let list_len = u16::from_be_bytes([data[0], data[1]]) as usize;
    if data.len() < 2 + list_len || list_len % 2 != 0 {
        return;
    }
    for chunk in data[2..2 + list_len].chunks_exact(2) {
        let group = u16::from_be_bytes([chunk[0], chunk[1]]);
        if !is_grease_u16(group) {
            out.push(group);
        }
    }
}

fn parse_ec_point_formats(data: &[u8], out: &mut Vec<u8>) {
    if data.is_empty() {
        return;
    }
    let list_len = data[0] as usize;
    if data.len() < 1 + list_len {
        return;
    }
    out.extend_from_slice(&data[1..1 + list_len]);
}

fn is_grease_u16(value: u16) -> bool {
    let hi = (value >> 8) as u8;
    let lo = value as u8;
    hi == lo && (hi & 0x0f) == 0x0a
}

fn join_u16(values: &[u16]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("-")
}

fn join_u8(values: &[u8]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("-")
}

fn u24_to_usize(bytes: &[u8]) -> usize {
    ((bytes[0] as usize) << 16) | ((bytes[1] as usize) << 8) | bytes[2] as usize
}

fn read_u8(buf: &[u8], cursor: &mut usize) -> Option<u8> {
    let val = *buf.get(*cursor)?;
    *cursor += 1;
    Some(val)
}

fn read_u16(buf: &[u8], cursor: &mut usize) -> Option<u16> {
    let b0 = *buf.get(*cursor)?;
    let b1 = *buf.get(*cursor + 1)?;
    *cursor += 2;
    Some(u16::from_be_bytes([b0, b1]))
}
