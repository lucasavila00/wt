use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

pub struct RejectProvider {
    pub address: std::net::SocketAddr,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    hold: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl RejectProvider {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let hold = Arc::new(AtomicBool::new(false));
        let holding = hold.clone();
        let worker = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let mut stream = stream.unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(line.starts_with("POST /v1/responses "), "{line}");
                let mut length = 0;
                loop {
                    line.clear();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<u64>().unwrap();
                    }
                }
                assert!(length > 0, "expected a bounded Responses request body");
                std::io::copy(&mut reader.take(length), &mut std::io::sink()).unwrap();
                seen.fetch_add(1, Ordering::SeqCst);
                while holding.load(Ordering::SeqCst) && !stopping.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                let body = r#"{"error":{"message":"intentional WT compatibility rejection","type":"invalid_request_error","code":"wt_ci_rejection"}}"#;
                let _ = write!(stream, "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        Self {
            address,
            requests,
            stop,
            hold,
            worker: Some(worker),
        }
    }

    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    pub fn hold(&self, value: bool) {
        self.hold.store(value, Ordering::SeqCst);
    }

    pub fn config(&self) -> String {
        format!("model_providers.wt_ci={{name='WT CI',base_url='http://{}/v1',wire_api='responses',requires_openai_auth=false,supports_websockets=false,request_max_retries=0,stream_max_retries=0}}", self.address)
    }
}

impl Drop for RejectProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
