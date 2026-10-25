use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

const ID: &str = "com.tezbar.native.gif-search";
struct Host {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Value>,
    serial: usize,
    calls: Vec<Value>,
    events: Vec<Value>,
}
impl Host {
    fn start(root: &Path, origin: Option<&str>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tezbar-gif-search"));
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .env("HOME", root)
            .env("USERPROFILE", root)
            .env_remove("TEZBAR_GIF_FIXTURE_ORIGIN");
        if let Some(origin) = origin {
            command.env("TEZBAR_GIF_FIXTURE_ORIGIN", origin);
        }
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::sync_channel(256);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                assert!(line.len() < 16 * 1024 * 1024);
                if tx.send(serde_json::from_str(&line).unwrap()).is_err() {
                    break;
                }
            }
        });
        let host = Self {
            child,
            input,
            output,
            serial: 0,
            calls: vec![],
            events: vec![],
        };
        let hello = host.receive();
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["id"], ID);
        assert_eq!(hello["protocol"], 1);
        host
    }
    fn receive(&self) -> Value {
        self.output
            .recv_timeout(Duration::from_secs(8))
            .expect("process frame timeout/EOF")
    }
    fn send(&mut self, frame: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{frame}").unwrap();
        input.flush().unwrap();
    }
    fn request(&mut self, method: &str, params: Value) -> String {
        self.serial += 1;
        let id = format!("h:{}", uuid::Uuid::new_v4());
        self.send(json!({"type":"request","id":id,"method":method,"params":params}));
        id
    }
    fn wait(&mut self, id: &str) -> Value {
        loop {
            let frame = self.receive();
            match frame["type"].as_str().unwrap() {
                "event" => self.events.push(frame),
                "request" => {
                    assert!(frame["id"].as_str().unwrap().starts_with("e:"));
                    let result = match frame["method"].as_str().unwrap() {
                        "host.clipboard.writeFiles" => {
                            let paths = frame["params"]["paths"].as_array().unwrap();
                            assert_eq!(paths.len(), 1);
                            let path = Path::new(paths[0].as_str().unwrap());
                            assert!(path.is_absolute());
                            let bytes = std::fs::read(path).unwrap();
                            assert!(bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"));
                            Value::Null
                        }
                        "host.clipboard.write" | "host.clipboard.pasteText" => {
                            assert!(frame["params"]["text"].is_string());
                            Value::Null
                        }
                        "host.clipboard.paste" => {
                            assert_eq!(frame["params"], json!({}));
                            Value::Null
                        }
                        "host.window.visibility" => {
                            assert_eq!(frame["params"], json!({"visible":false}));
                            Value::Null
                        }
                        "host.shell.open" => {
                            assert!(frame["params"]["target"].is_string());
                            Value::Null
                        }
                        "host.shell.reveal" => {
                            assert!(frame["params"]["path"].is_string());
                            Value::Null
                        }
                        method => panic!("unexpected host method {method}"),
                    };
                    self.send(json!({"type":"response","id":frame["id"],"result":result}));
                    self.calls.push(frame);
                }
                "response" => {
                    assert_eq!(frame["id"], id);
                    return frame;
                }
                kind => panic!("unexpected frame {kind}"),
            }
        }
    }
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = self.request(method, params);
        self.wait(&id)
    }
    fn initialize(&mut self, root: &Path, migrations: Value) {
        let reply=self.rpc("initialize", json!({"protocol":1,"extensionId":ID,"platform":if cfg!(windows){"Windows"}else{"macOS"},
            "packageDir":root,"dataDir":root.join("data"),"tempDir":root.join("temp"),"reason":"fixture",
            "grantedCapabilities":["clipboard","shell","window"],"migrationPaths":migrations}));
        assert_eq!(reply["result"]["ready"], true);
    }
    fn open(&mut self, session: &str, command: &str, prefs: Value) -> Value {
        self.rpc("session.open",json!({"sessionId":session,"commandName":command,"preferences":prefs,"argumentValues":{}}))
    }
    fn call(&mut self, session: &str, operation: &str, payload: Value) -> Value {
        self.rpc(
            "session.call",
            json!({"sessionId":session,"operation":operation,"payload":payload}),
        )
    }
    fn action(&mut self, session: &str, id: &str) -> Value {
        self.call(session, "extension:invoke-action", json!({"actionId":id}))
    }
    fn settled(&mut self, session: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let response = self.call(session, "extension:refresh-session", json!({}));
            assert!(response.get("error").is_none(), "{response}");
            if response["result"]["mode"] == "view"
                && response["result"]["root"]["props"]["isLoading"] == false
            {
                return response["result"].clone();
            }
            assert!(Instant::now() < deadline, "never settled: {response}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn shutdown(&mut self) {
        assert!(self.rpc("shutdown", json!({})).get("result").is_some());
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "shutdown blocked with stdin open"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn snapshot(id: &str) -> Value {
    json!({"service":"giphy","id":id,"title":"Hello GIF","url":format!("https://giphy.com/gifs/{id}"),
        "gif_url":format!("https://media.giphy.com/{id}.gif?tracking=1"),"download_url":format!("https://media.giphy.com/{id}.gif"),
        "download_name":"hello-gif.gif","preview":format!("https://media.giphy.com/{id}.gif"),"large_preview":format!("https://media.giphy.com/{id}.gif"),"metadata":{}})
}
fn seed(root: &Path) -> std::path::PathBuf {
    let path = root.join("native-migration.json");
    std::fs::write(&path, json!({"service":"favorites","giphy-favs":["a"],"giphy-recent":["a"],"gifSearchItems":{"giphy:a":snapshot("a")},"token":"fixture-do-not-copy"}).to_string()).unwrap();
    path
}
fn has_item(view: &Value, id: &str) -> bool {
    view["root"]["children"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|section| {
            section["children"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|item| item["props"]["id"] == id)
        })
}

#[test]
fn offline_process_migration_actions_preferences_and_session_isolation() {
    let root = tempfile::tempdir().unwrap();
    let path = seed(root.path());
    let original = std::fs::read(&path).unwrap();
    let mut host = Host::start(root.path(), None);
    host.initialize(root.path(), json!({"gif-search.native-v2-store":path}));
    let view = host.open(
        "f",
        "favorites",
        json!({"defaultAction":"copyGifUrl","gridItemSize":"large"}),
    );
    assert!(has_item(&view["result"], "giphy:a"));
    assert_eq!(view["result"]["root"]["props"]["columns"], 3);
    assert_eq!(
        view["result"]["actions"][0]["id"],
        "gif-search:copyGifUrl:giphy%3Aa"
    );
    assert!(has_item(
        &host.open("r", "recents", json!({}))["result"],
        "giphy:a"
    ));
    let response=host.call("f","extension:invoke-action",json!({"actionId":"gif-search:copyGifUrl:giphy%3Aa","preferences":{"defaultAction":"copyFile","gridItemSize":"small"},"sessionId":"r"}));
    assert_eq!(response["result"]["root"]["props"]["columns"], 3);
    assert_eq!(host.calls[0]["method"], "host.clipboard.write");
    assert_eq!(
        host.calls[0]["params"],
        json!({"text":"https://media.giphy.com/a.gif"})
    );
    assert!(!has_item(
        &host.call(
            "f",
            "extension:search-text-changed",
            json!({"searchText":"absent"})
        )["result"],
        "giphy:a"
    ));
    assert!(has_item(
        &host.call("r", "extension:refresh-session", json!({}))["result"],
        "giphy:a"
    ));
    assert!(host
        .rpc("session.dispose", json!({"sessionId":"f"}))
        .get("result")
        .is_some());
    assert!(host
        .call("f", "extension:refresh-session", json!({}))
        .get("error")
        .is_some());
    let store = std::fs::read_to_string(root.path().join("data/gif-search/storage.json")).unwrap();
    assert!(!store.contains("fixture-do-not-copy"));
    assert_eq!(std::fs::read(path).unwrap(), original);
    host.shutdown();
}

#[test]
fn process_rejects_corruption_and_ignores_ungranted_legacy_paths() {
    let root = tempfile::tempdir().unwrap();
    let store = root.path().join("data/gif-search/storage.json");
    std::fs::create_dir_all(store.parent().unwrap()).unwrap();
    std::fs::write(&store, b"{corrupt").unwrap();
    let mut host = Host::start(root.path(), None);
    host.initialize(root.path(), json!({}));
    assert!(host
        .open("broken", "favorites", json!({}))
        .get("error")
        .is_some());
    assert_eq!(std::fs::read(&store).unwrap(), b"{corrupt");
    host.shutdown();
    let root = tempfile::tempdir().unwrap();
    let path = seed(root.path());
    let mut host = Host::start(root.path(), None);
    host.initialize(root.path(), json!({"unrelated-secret-config":path}));
    assert!(!has_item(
        &host.open("empty", "favorites", json!({}))["result"],
        "giphy:a"
    ));
    host.shutdown();
}

#[test]
fn malformed_jsonl_process_exits_without_protocol_success() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tezbar-gif-search"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{invalid\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let lines = String::from_utf8(output.stdout).unwrap();
    assert_eq!(lines.lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(lines.trim()).unwrap()["type"],
        "hello"
    );
}

#[cfg(feature = "fixture-http")]
mod network {
    use super::*;
    use std::{
        net::{TcpListener, TcpStream},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
    };
    struct Server {
        origin: String,
        stop: Arc<AtomicBool>,
        requests: Arc<Mutex<Vec<String>>>,
        cancelled: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    fn gif(id: &str) -> Value {
        json!({"id":id,"title":format!("GIF {id}"),"url":format!("https://giphy.com/gifs/{id}"),"images":{"original":{"url":format!("https://media.giphy.com/{id}.gif"),"width":"4","height":"2"},"preview_gif":{"url":format!("https://media.giphy.com/{id}.gif")}}})
    }
    fn bytes() -> Vec<u8> {
        let mut bytes = vec![];
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Finite(2))
                .unwrap();
            for delay in [100, 200] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 255])),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(delay, 1),
                    ))
                    .unwrap();
            }
        }
        bytes
    }
    impl Server {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let requests = Arc::new(Mutex::new(vec![]));
            let cancelled = Arc::new(AtomicBool::new(false));
            let (s, r, c) = (stop.clone(), requests.clone(), cancelled.clone());
            let thread = std::thread::spawn(move || {
                let mut workers = vec![];
                while !s.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let (r, c, s) = (r.clone(), c.clone(), s.clone());
                            workers.push(std::thread::spawn(move || serve(stream, r, c, s)));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("{error}"),
                    }
                }
                for worker in workers {
                    worker.join().unwrap();
                }
            });
            Self {
                origin,
                stop,
                requests,
                cancelled,
                thread: Some(thread),
            }
        }
        fn wait_request(&self, needle: &str) {
            let deadline = Instant::now() + Duration::from_secs(4);
            while !self
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.contains(needle))
            {
                assert!(Instant::now() < deadline, "missing {needle}");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.thread.take().unwrap().join().unwrap();
        }
    }
    fn serve(
        mut stream: TcpStream,
        requests: Arc<Mutex<Vec<String>>>,
        cancelled: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
    ) {
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut raw = Vec::new();
        let mut byte = [0u8; 1];
        while raw.len() < 8192 && !raw.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => raw.push(byte[0]),
                _ => return,
            }
        }
        let request = String::from_utf8(raw).unwrap();
        let path = request.split_whitespace().nth(1).unwrap().to_string();
        requests.lock().unwrap().push(path.clone());
        if path.contains("q=slow") {
            let deadline = Instant::now() + Duration::from_secs(7);
            while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
                match stream.read(&mut byte) {
                    Ok(0) => {
                        cancelled.store(true, Ordering::SeqCst);
                        return;
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    _ => return,
                }
            }
            return;
        }
        let (status, body, declared) = if path.contains("q=quota") {
            (429, b"private-provider-error".to_vec(), None)
        } else if path.contains("q=oversize") {
            (200, vec![], Some(8 * 1024 * 1024 + 1))
        } else if path.contains("q=malformed") {
            (200, b"{invalid".to_vec(), None)
        } else if path.ends_with(".gif") {
            (200, bytes(), None)
        } else if path.starts_with("/api/klipy") {
            let id = if path.contains("pos=") { "k2" } else { "k1" };
            (200,json!({"results":[{"id":id,"title":"Klipy","itemurl":"https://klipy.com/gifs/cat","media_formats":{"gif":{"url":"https://media.klipy.com/cat.gif","dims":[4,2]}}}],"next":if id=="k1"{"next+/=&"}else{""}}).to_string().into_bytes(),None)
        } else if path.starts_with("/search") {
            (
                200,
                json!({"results":[{"fields":{"fileid":"02x03-finer","text":"Finer"}}]})
                    .to_string()
                    .into_bytes(),
                None,
            )
        } else {
            let id = if path.contains("offset=1") {
                "b"
            } else if path.contains("q=new") {
                "new"
            } else {
                "a"
            };
            (
                200,
                json!({"data":[gif(id)],"pagination":{"total_count":2}})
                    .to_string()
                    .into_bytes(),
                None,
            )
        };
        let header = format!(
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            declared.unwrap_or(body.len())
        );
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(&body);
    }

    #[test]
    fn executable_provider_paging_errors_and_cancellation_use_local_http() {
        let root = tempfile::tempdir().unwrap();
        let server = Server::start();
        let mut host = Host::start(root.path(), Some(&server.origin));
        host.initialize(root.path(), json!({}));
        assert_eq!(
            host.open("s", "search", json!({"maxResults":"1","giphyLocale":"ru"}))["result"]
                ["root"]["props"]["isLoading"],
            true
        );
        assert!(has_item(&host.settled("s"), "giphy:a"));
        host.call("s", "extension:load-more", json!({}));
        let page = host.settled("s");
        assert!(has_item(&page, "giphy:a"));
        assert!(has_item(&page, "giphy:b"));
        server.wait_request("offset=1");
        server.wait_request("lang=ru");
        for provider in ["giphy-clips", "klipy", "finergifs"] {
            host.call(
                "s",
                "extension:invoke-action",
                json!({"actionId":"gif-search:service","formValues":{"value":provider}}),
            );
            let mut page = host.settled("s");
            if provider == "finergifs" {
                host.call(
                    "s",
                    "extension:search-text-changed",
                    json!({"searchText":"office"}),
                );
                page = host.settled("s");
            }
            assert!(has_item(
                &page,
                &format!(
                    "{provider}:{}",
                    if provider == "finergifs" {
                        "02x03-finer"
                    } else if provider == "klipy" {
                        "k1"
                    } else {
                        "a"
                    }
                )
            ));
            if provider == "klipy" {
                host.call("s", "extension:load-more", json!({}));
                assert!(has_item(&host.settled("s"), "klipy:k2"));
                server.wait_request("pos=next%2B%2F%3D%26");
            }
        }
        server.wait_request("type=videos");
        host.call(
            "s",
            "extension:invoke-action",
            json!({"actionId":"gif-search:service","formValues":{"value":"giphy"}}),
        );
        host.settled("s");
        for query in ["quota", "oversize", "malformed"] {
            host.call(
                "s",
                "extension:search-text-changed",
                json!({"searchText":query}),
            );
            let view = host.settled("s");
            assert!(view["root"]["props"]["error"].is_string());
            assert!(!view.to_string().contains("private-provider-error"));
        }
        host.call(
            "s",
            "extension:search-text-changed",
            json!({"searchText":"slow"}),
        );
        server.wait_request("q=slow");
        host.call(
            "s",
            "extension:search-text-changed",
            json!({"searchText":"new"}),
        );
        assert!(has_item(&host.settled("s"), "giphy:new"));
        let deadline = Instant::now() + Duration::from_secs(3);
        while !server.cancelled.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "HTTP not cancelled");
            std::thread::sleep(Duration::from_millis(5));
        }
        host.call(
            "s",
            "extension:search-text-changed",
            json!({"searchText":"slow-again"}),
        );
        server.wait_request("q=slow-again");
        host.rpc("session.dispose", json!({"sessionId":"s"}));
        host.shutdown();
    }

    #[test]
    fn executable_media_clipboard_square_cache_and_download_are_fixture_only() {
        let root = tempfile::tempdir().unwrap();
        let server = Server::start();
        let path = seed(root.path());
        let mut host = Host::start(root.path(), Some(&server.origin));
        host.initialize(root.path(), json!({"gif-search.native-v2-store":path}));
        let downloads = root.path().join("Downloads");
        std::fs::create_dir(&downloads).unwrap();
        host.open(
            "f",
            "favorites",
            json!({"downloadPath":downloads,"hideFilename":true}),
        );
        for action in [
            "copyFile",
            "pasteFile",
            "copySquareGif",
            "pasteSquareGif",
            "pasteGifUrl",
            "pasteGifMarkdown",
            "copyGifMarkdown",
            "copyPageUrl",
            "openUrlInBrowser",
            "viewDetails",
            "downloadFile",
        ] {
            let reply = host.action("f", &format!("gif-search:{action}:giphy%3Aa"));
            assert!(reply.get("error").is_none(), "{reply}");
        }
        assert_eq!(std::fs::read(downloads.join("gif.gif")).unwrap(), bytes());
        assert_eq!(
            server
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.ends_with("/a.gif"))
                .count(),
            3,
            "original cached file reused; each square conversion downloads original"
        );
        let square = host
            .calls
            .iter()
            .filter(|c| c["method"] == "host.clipboard.writeFiles")
            .nth(2)
            .unwrap();
        use image::{AnimationDecoder, ImageDecoder};
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(
            std::fs::read(square["params"]["paths"][0].as_str().unwrap()).unwrap(),
        ))
        .unwrap();
        assert_eq!(decoder.dimensions(), (2, 2));
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].delay().numer_denom_ms(), (200, 1));
        assert!(host
            .calls
            .iter()
            .any(|c| c["method"] == "host.clipboard.paste"));
        host.action("f", "gif-search:open-download");
        host.action("f", "gif-search:reveal-download");
        host.shutdown();
    }
    #[test]
    fn executable_hydrates_id_only_snapshots_and_eof_cancels_active_http() {
        let root = tempfile::tempdir().unwrap();
        let server = Server::start();
        let legacy = root.path().join("legacy.json");
        std::fs::write(
            &legacy,
            json!({"giphy-favs":"[\"a\"]", "secret":"exclude-me"}).to_string(),
        )
        .unwrap();
        let mut host = Host::start(root.path(), Some(&server.origin));
        // Constructing/initializing the process never starts a provider request.
        host.initialize(root.path(), json!({"gif-search.legacy-storage":legacy}));
        assert!(server.requests.lock().unwrap().is_empty());
        let view = host.open("f", "favorites", json!({}));
        assert_eq!(view["result"]["root"]["props"]["isLoading"], true);
        assert!(has_item(&host.settled("f"), "giphy:a"));
        server.wait_request("ids=a");
        let store =
            std::fs::read_to_string(root.path().join("data/gif-search/storage.json")).unwrap();
        let store: Value = serde_json::from_str(&store).unwrap();
        assert_eq!(store["gifSearchItems"]["giphy:a"]["id"], "a");
        assert!(store.get("secret").is_none());
        host.open("s", "search", json!({}));
        host.settled("s");
        host.call(
            "s",
            "extension:search-text-changed",
            json!({"searchText":"slow"}),
        );
        server.wait_request("q=slow");
        drop(host.input.take());
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(status) = host.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "EOF left a session job/process alive"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        while !server.cancelled.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "EOF did not abort HTTP socket");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
