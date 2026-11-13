use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use veyra_net::*;

/// Minimal loopback HTTP server: answers every request with `body` (honours `Range: bytes=N-`).
async fn serve(body: Vec<u8>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap(); let port = l.local_addr().unwrap().port();
    tokio::spawn(async move { loop {
        let Ok((mut s, _)) = l.accept().await else { return }; let body = body.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096]; let n = s.read(&mut buf).await.unwrap_or(0); let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
            let start = req.lines().find_map(|l| l.strip_prefix("range: bytes=")).and_then(|r| r.trim_end_matches('-').parse::<usize>().ok());
            let (code, slice) = match start { Some(o) if o < body.len() => ("206 Partial Content", &body[o..]), _ => ("200 OK", &body[..]) };
            let _ = s.write_all(format!("HTTP/1.1 {code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", slice.len()).as_bytes()).await; let _ = s.write_all(slice).await;
        });
    }});
    format!("http://127.0.0.1:{port}/x")
}
fn client() -> NetClient { NetClient::new(Config::default()).unwrap() }

#[tokio::test] async fn local_engine_traffic_is_counted_as_local_and_bytes_are_measured() {
    let url = serve(vec![7u8; 1000]).await; let c = client();
    let mut t = c.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::POST, &url, &[], Some(vec![1; 40]), None).await.unwrap();
    let mut got = 0; while let Some(b) = t.chunk().await.unwrap() { got += b.len(); } t.finish(None);
    assert_eq!(got, 1000);
    let s = c.ledger.summary();
    assert_eq!((s.requests, s.local_requests, s.external_requests, s.in_flight, s.errors), (1, 1, 0, 0, 0));
    assert_eq!((s.bytes_sent, s.bytes_received), (40, 1000)); assert!(s.hosts.is_empty(), "loopback is not an external host");
    let e = &c.ledger.entries()[0]; assert_eq!(e.host, "127.0.0.1"); assert!(e.local && !e.in_flight && e.status == Some(200));
}
#[tokio::test] async fn user_only_purposes_are_refused_when_started_internally_and_nothing_is_sent() {
    let c = client();
    for p in [Purpose::ModelDownload, Purpose::WebSearch, Purpose::Update] {
        let r = c.request(p, Initiator::Internal, reqwest::Method::GET, "https://example.invalid/", &[], None, None).await;
        assert!(matches!(r, Err(NetError::Blocked(_))), "{p:?}");
    }
    assert_eq!(c.ledger.summary().requests, 0, "a blocked request must not even be recorded as traffic");
}
#[tokio::test] async fn plain_http_is_refused_for_external_hosts_and_loopback_is_reserved_for_the_engine() {
    let c = client();
    assert!(matches!(c.request(Purpose::WebSearch, Initiator::User, reqwest::Method::GET, "http://example.com/", &[], None, None).await, Err(NetError::InsecureScheme)));
    let url = serve(vec![1]).await;
    assert!(matches!(c.request(Purpose::WebSearch, Initiator::User, reqwest::Method::GET, &url, &[], None, None).await, Err(NetError::Blocked(_))), "an external purpose must not target loopback");
    assert!(matches!(c.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::GET, "https://example.com/", &[], None, None).await, Err(NetError::Blocked(_))), "the engine must not reach the Internet");
}
#[tokio::test] async fn connection_failure_is_classified_and_recorded() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap(); let port = l.local_addr().unwrap().port(); drop(l);
    let c = client();
    let r = c.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::GET, &format!("http://127.0.0.1:{port}/"), &[], None, Some(Duration::from_secs(10))).await;
    // Windows retries a refused loopback connection for ~2 s, so the connect timeout can fire first: both are honest classifications of "could not connect".
    assert!(matches!(r, Err(NetError::Failed(ErrorClass::Connect | ErrorClass::Timeout, _))), "{:?}", r.err());
    let s = c.ledger.summary(); assert_eq!((s.requests, s.errors, s.in_flight), (1, 1, 0));
}
#[tokio::test] async fn dropped_response_is_marked_cancelled_not_left_in_flight() {
    let url = serve(vec![0; 10]).await; let c = client();
    { let _t = c.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::GET, &url, &[], None, None).await.unwrap(); }
    let e = &c.ledger.entries()[0]; assert!(!e.in_flight); assert_eq!(e.error, Some(ErrorClass::Cancelled));
}
#[tokio::test] async fn range_header_is_forwarded_for_resumable_downloads() {
    let url = serve((0..100u8).collect()).await; let c = client();
    let mut t = c.request(Purpose::LocalEngine, Initiator::Internal, reqwest::Method::GET, &url, &[("Range", "bytes=60-".into())], None, None).await.unwrap();
    assert_eq!(t.status(), 206); let mut v = vec![]; while let Some(b) = t.chunk().await.unwrap() { v.extend_from_slice(&b); } t.finish(None);
    assert_eq!(v, (60..100u8).collect::<Vec<_>>());
}
#[tokio::test] async fn ledger_is_bounded_and_clearable() {
    let url = serve(vec![0; 1]).await; let c = client();
    for _ in 0..3 { c.get_bytes(Purpose::LocalEngine, Initiator::Internal, &url, 100).await.unwrap(); }
    assert_eq!(c.ledger.summary().requests, 3); c.ledger.clear(); assert_eq!(c.ledger.summary().requests, 0);
    assert!(c.get_bytes(Purpose::LocalEngine, Initiator::Internal, &url, 0).await.is_err(), "size cap is enforced");
}
