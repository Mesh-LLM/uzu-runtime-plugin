//! Decode upstream HTTP framing before exposing a connection-delimited body to Mesh.
//! Older Mesh hosts do not preserve chunk boundaries correctly for every SSE frame.
use anyhow::{Result, bail, ensure};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub async fn serve(listener: TcpListener, backend: String) -> Result<()> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    loop {
        let (stream, _) = listener.accept().await?;
        let client = client.clone();
        let backend = backend.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                relay(stream, &client, &backend),
            )
            .await;
        });
    }
}

async fn relay(mut stream: TcpStream, client: &reqwest::Client, backend: &str) -> Result<()> {
    let mut request = Vec::new();
    let header_end = loop {
        if let Some(offset) = request.windows(4).position(|b| b == b"\r\n\r\n") {
            break offset + 4;
        }
        ensure!(request.len() < 65536, "request headers too large");
        let mut buf = [0; 4096];
        let size = stream.read(&mut buf).await?;
        ensure!(size > 0, "truncated request");
        request.extend_from_slice(&buf[..size]);
    };
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Request::new(&mut headers);
    ensure!(
        parsed.parse(&request[..header_end])?.is_complete(),
        "incomplete headers"
    );
    let method: reqwest::Method = parsed.method.unwrap_or("").parse()?;
    let path = parsed.path.unwrap_or("");
    ensure!(
        path.starts_with("/v1/") && !path.starts_with("//"),
        "invalid path"
    );
    let mut length = None;
    let mut content_type = None;
    for header in parsed.headers.iter() {
        if header.name.eq_ignore_ascii_case("transfer-encoding") {
            bail!("chunked requests are not supported");
        }
        if header.name.eq_ignore_ascii_case("content-length") {
            ensure!(length.is_none(), "duplicate content length");
            length = Some(std::str::from_utf8(header.value)?.parse::<usize>()?);
        }
        if header.name.eq_ignore_ascii_case("content-type") {
            content_type = Some(std::str::from_utf8(header.value)?.to_string());
        }
    }
    let length = length.unwrap_or(0);
    ensure!(length <= 16 * 1024 * 1024, "request too large");
    let url = format!("{backend}{path}");
    let mut body = request[header_end..].to_vec();
    ensure!(body.len() <= length, "pipelined requests are not supported");
    while body.len() < length {
        let mut buf = vec![0; (length - body.len()).min(8192)];
        let size = stream.read(&mut buf).await?;
        ensure!(size > 0, "truncated body");
        body.extend_from_slice(&buf[..size]);
    }
    let mut builder = client.request(method, url).body(body);
    if let Some(content_type) = content_type {
        builder = builder.header("Content-Type", content_type);
    }
    let mut response = tokio::select! {
        response = builder.send() => response?,
        result = stream.read_u8() => { let _ = result; return Ok(()); }
    };
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");
    let header = format!(
        "HTTP/1.0 {} {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("Response"),
        content_type
    );
    stream.write_all(header.as_bytes()).await?;
    // A disconnect drops the response immediately, cancelling upstream generation.
    loop {
        let next = tokio::select! {
            data = response.chunk() => data?,
            result = stream.read_u8() => { let _ = result; return Ok(()); }
        };
        let Some(bytes) = next else {
            break;
        };
        stream.write_all(&bytes).await?;
    }
    stream.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn fragmented_chunked_sse_is_decoded_without_losing_bytes() {
        let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", backend.local_addr().unwrap());
        let fixture = tokio::spawn(async move {
            let (mut socket, _) = backend.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n3\r\ndat\r\n5\r\na: {}\r\n2\r\n\n\n\r\n0\r\n\r\n").await.unwrap();
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap();
        let adapter = tokio::spawn(serve(listener, address));
        let response = reqwest::get(format!("http://{port}/v1/models"))
            .await
            .unwrap();
        assert!(!response.headers().contains_key("transfer-encoding"));
        assert_eq!(response.text().await.unwrap(), "data: {}\n\n");
        fixture.await.unwrap();
        adapter.abort();
    }
}
