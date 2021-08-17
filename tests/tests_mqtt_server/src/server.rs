use once_cell::sync::OnceCell;
use std::{
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

// Child doesn't implement `Drop` therefore we have to shutdown the process by hand.
// Use of `static mut` requires some unsafe code.
static mut SERVER: OnceCell<Child> = OnceCell::new();
static CALLERS_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn start_server(port: u16) {
    // `unsafe` has to be used due to `SERVER` being `static mut`.
    unsafe {
        let _server_child_handle = SERVER.get_or_init(|| spawn_server_process(port));
    }

    CALLERS_COUNTER.fetch_add(1, Ordering::Relaxed);
}

fn try_stop_server() {
    CALLERS_COUNTER.fetch_sub(1, Ordering::Relaxed);

    if CALLERS_COUNTER.load(Ordering::Relaxed) == 0 {
        // `unsafe` has to be used due to `SERVER` being `static mut`,
        // race between callers may occur in rare cases when a late caller will queue for the atomic operation after previous caller brought the count to 0.
        unsafe {
            // TODO: Remove unwrap.
            let server_child_handle = SERVER.get_mut().unwrap();

            // TODO: Use the result.
            let _ = server_child_handle.kill();
        }
    }
}

fn spawn_server_process(port: u16) -> Child {
    Command::new("mosquitto")
        .args(&["-p", port.to_string().as_str()])
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        // TODO: Remove unwrap.
        .unwrap()
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
