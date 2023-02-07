use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::channel,
    time::Duration,
};
use tide_websockets::Message;

use crate::{PATH, WS_CLIENTS};

async fn broadcast() {
    for (_, conn) in WS_CLIENTS.lock().await.iter() {
        conn.send(Message::Text(String::new())).await.unwrap();
    }
}

pub async fn watch(path: String) {
    let abs_path = match fs::canonicalize(path.clone()) {
        Ok(path) => path,
        Err(err) => {
            log::error!("Failed to get absolute path of `{}`: {}", path, err);
            return;
        }
    };
    PATH.set(abs_path.clone()).unwrap();
    let abs_path_str = match abs_path.clone().into_os_string().into_string() {
        Ok(path_str) => path_str,
        Err(_) => {
            log::error!("Failed to parse path to string for `{:?}`", abs_path);
            return;
        }
    };
    log::info!("Listening on {}", abs_path_str);
    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::from_millis(100)).unwrap();
    match watcher.watch(abs_path.clone(), RecursiveMode::Recursive) {
        Ok(_) => {}
        Err(err) => log::warn!("Watcher: {}", err),
    }

    loop {
        use DebouncedEvent::*;
        let recv = rx.recv();
        match recv {
            Ok(event) => match event {
                Create(path) => {
                    log::info!("[CREATE] {}", strip_prefix(path, &abs_path));
                    broadcast().await;
                }
                Write(path) => {
                    log::info!("[UPDATE] {}", strip_prefix(path, &abs_path));
                    broadcast().await;
                }
                Remove(path) => {
                    log::info!("[REMOVE] {}", strip_prefix(path, &abs_path));
                    broadcast().await;
                }
                Rename(from, to) => {
                    log::info!(
                        "[RENAME] {} -> {}",
                        strip_prefix(from, &abs_path),
                        strip_prefix(to, &abs_path)
                    );
                    broadcast().await;
                }
                Error(err, _) => log::error!("{}", err),
                _ => {}
            },
            Err(err) => log::error!("{}", err),
        }
    }
}

fn strip_prefix(path: PathBuf, prefix: &Path) -> String {
    path.strip_prefix(prefix)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}
