#![allow(deprecated)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]

//! A simple WebRTC streaming server. It streams video and audio from a file to a browser client.

use anyhow::Result;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use lazy_static::lazy_static;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

lazy_static! {
    static ref SDP_CHAN_TX_MUTEX: Arc<Mutex<Option<mpsc::Sender<String>>>> =
        Arc::new(Mutex::new(None));
}

async fn remote_handler(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/sdp") => {
            let sdp_str = match std::str::from_utf8(&hyper::body::to_bytes(req.into_body()).await?)
            {
                Ok(s) => s.to_owned(),
                Err(err) => panic!("{}", err),
            };
            {
                let sdp_chan_tx = SDP_CHAN_TX_MUTEX.lock().await;
                if let Some(tx) = &*sdp_chan_tx {
                    let _ = tx.send(sdp_str).await;
                }
            }

            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        }

        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

/// Bind to a port and serve sdp.
pub async fn http_sdp_server(port: u16) -> mpsc::Receiver<String> {
    let (sdp_chan_tx, sdp_chan_rx) = mpsc::channel::<String>(1);
    {
        let mut tx = SDP_CHAN_TX_MUTEX.lock().await;
        *tx = Some(sdp_chan_tx);
    }

    tokio::spawn(async move {
        let addr = SocketAddr::from_str(&format!("0.0.0.0:{port}")).unwrap();
        let service =
            make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(remote_handler)) });
        let server = Server::bind(&addr).serve(service);

        if let Err(e) = server.await {
            eprintln!("server error: {e}");
        }
    });

    sdp_chan_rx
}

/// Helper function to read input base64 data.
pub fn must_read_stdin() -> Result<String> {
    let mut line = String::new();

    std::io::stdin().read_line(&mut line)?;
    line = line.trim().to_owned();
    println!();

    Ok(line)
}

/// Encode base64 wrapper function.
pub fn encode(b: &str) -> String {
    base64::encode(b.as_bytes())
}

/// Decode base64 wrapper function.
pub fn decode(s: &str) -> Result<String> {
    let b = base64::decode(s.as_bytes())?;
    let s = String::from_utf8(b)?;
    Ok(s)
}
