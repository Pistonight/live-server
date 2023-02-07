use async_std::fs;
use async_std::prelude::*;
use once_cell::sync::OnceCell;
use tide::{listener::Listener, Body, Request, Response, StatusCode};
use tide_websockets::WebSocket;
use uuid::Uuid;

use crate::{HOST, PATH, WS_CLIENTS};

pub static SCRIPT: OnceCell<String> = OnceCell::new();

pub async fn serve(port: u16) {
    // Here we can call `unwrap()` safely because we have set it
    // before calling `serve()`
    let host = HOST.get().unwrap();

    // Here we can call `unwrap()` safely because we have set it
    // before calling `serve()`
    let mut port = port;
    let mut listener = create_listener(host, &mut port).await;
    init_ws_script(port);

    let url = format!("http://{}:{}/", host, port);
    log::info!("Listening on {}", url);
    listener.accept().await.unwrap();
}

fn create_server() -> tide::Server<()> {
    let mut app = tide::new();
    app.at("/").get(static_assets);
    app.at("/*").get(static_assets);
    app.at("/live-server-ws")
        .get(WebSocket::new(|_request, mut stream| async move {
            let uuid = Uuid::new_v4();
            // Add the connection to clients when opening a new connection
            WS_CLIENTS.lock().await.insert(uuid, stream.clone());
            // Waiting for the connection to be closed
            while let Some(Ok(_)) = stream.next().await {}
            // Remove the connection from clients when it is closed
            WS_CLIENTS.lock().await.remove(&uuid);
            Ok(())
        }));
    app
}

async fn create_listener(host: &String, port: &mut u16) -> impl Listener<()> {
    // Loop until the port is available
    loop {
        let app = create_server();
        match app.bind(format!("{}:{}", host, port)).await {
            Ok(listener) => break listener,
            Err(err) => {
                if let std::io::ErrorKind::AddrInUse = err.kind() {
                    log::warn!("Port {} is already in use", port);
                    *port += 1;
                } else {
                    log::error!("Failed to listen on {}:{}: {}", host, port, err);
                }
            }
        }
    }
}

fn init_ws_script(port: u16) {
    let script = format!(
        include_str!("scripts/websocket.html"),
        HOST.get().unwrap(),
        port
    );
    SCRIPT.set(script).unwrap();
}

async fn static_assets(req: Request<()>) -> tide::Result {
    // Get the path and mime of the static file.
    let mut path = req.url().path().to_string();
    path = if path.ends_with('/') {
        format!("{}{}index.html", PATH.get().unwrap().display(), path)
    } else {
        format!("{}{}", PATH.get().unwrap().display(), path)
    };
    let mime = mime_guess::from_path(&path).first_or_text_plain();

    // Read the file.
    let mut file = match fs::read(&path).await {
        Ok(file) => file,
        Err(err) => {
            log::error!("{}", err);
            return Err(tide::Error::new(StatusCode::NotFound, err));
        }
    };

    // Construct the response.
    if mime == "text/html" {
        let text = match String::from_utf8(file) {
            Ok(text) => text,
            Err(err) => {
                log::error!("{}", err);
                return Err(tide::Error::from_str(StatusCode::InternalServerError, err));
            }
        };
        let script = SCRIPT.get().unwrap();
        file = format!("{}{}", text, script).into_bytes();
    }
    let mut response: Response = Body::from_bytes(file).into();
    response.set_content_type(mime.to_string().as_str());

    Ok(response)
}
