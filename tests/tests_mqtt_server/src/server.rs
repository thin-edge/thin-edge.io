use once_cell::sync::OnceCell;
use std::{
    net::TcpStream,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    thread::sleep,
    time::Duration,
};

static SERVER: OnceCell<Mutex<Child>> = OnceCell::new();
static CALLERS_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn start_server(port: u16) {
    let _server_child_handle = SERVER.get_or_init(|| Mutex::new(spawn_server_process(port)));

    CALLERS_COUNTER.fetch_add(1, Ordering::Relaxed);
}

fn try_stop_server() {
    CALLERS_COUNTER.fetch_sub(1, Ordering::Relaxed);

    if CALLERS_COUNTER.load(Ordering::Relaxed) == 0 {
        // race between callers may occur in rare cases when a late caller will queue for the atomic operation after previous caller brought the count to 0.
        // TODO: Remove unwrap.
        let server_child_handle = SERVER.get().unwrap();

        // TODO: Use the result.
        let _ = server_child_handle.lock().unwrap().kill();
    }
}

fn spawn_server_process(port: u16) -> Child {
    let child = Command::new("/usr/sbin/mosquitto")
        .args(&["-p", port.to_string().as_str()])
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        // TODO: Remove unwrap.
        .unwrap();

    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        sleep(Duration::from_millis(100));
    }

    child
}

pub struct TestsMqttServer {}

impl TestsMqttServer {
    pub fn new_with_port(port: u16) -> Self {
        start_server(port);

        Self {}
    }
}

impl Drop for TestsMqttServer {
    fn drop(&mut self) {
        try_stop_server();
    }
}
