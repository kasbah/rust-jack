use std::{ptr, thread, time, mem};
use std::sync::Mutex;

use prelude::*;

#[derive(Debug, Default)]
pub struct Counter {
    pub process_return_val: JackControl,
    pub induce_xruns: bool,
    pub thread_init_count: Mutex<usize>,
    pub frames_processed: usize,
    pub buffer_size_change_history: Vec<JackFrames>,
    pub registered_client_history: Vec<String>,
    pub unregistered_client_history: Vec<String>,
    pub port_register_history: Vec<JackPortId>,
    pub port_unregister_history: Vec<JackPortId>,
    pub xruns_count: usize,
}

impl NotificationHandler for Counter {
    fn thread_init(&self, _: &Client) {
        *self.thread_init_count.lock().unwrap() += 1;
    }

    fn buffer_size(&mut self, _: &Client, size: JackFrames) -> JackControl {
        self.buffer_size_change_history.push(size);
        JackControl::Continue
    }

    fn client_registration(&mut self, _: &Client, name: &str, is_registered: bool) {
        match is_registered {
            true => self.registered_client_history.push(name.to_string()),
            false => self.unregistered_client_history.push(name.to_string()),
        }
    }

    fn port_registration(&mut self, _: &Client, pid: JackPortId, is_registered: bool) {
        match is_registered {
            true => self.port_register_history.push(pid),
            false => self.port_unregister_history.push(pid),
        }
    }

    fn xrun(&mut self, _: &Client) -> JackControl {
        self.xruns_count += 1;
        JackControl::Continue
    }
}

impl ProcessHandler for Counter {
    fn process(&mut self, _: &Client, ps: &ProcessScope) -> JackControl {
        self.frames_processed += ps.n_frames() as usize;
        if self.induce_xruns {
            thread::sleep(time::Duration::from_millis(400));
        }
        JackControl::Continue
    }
}

fn open_test_client(name: &str) -> Client {
    Client::new(name, client_options::NO_START_SERVER).unwrap().0
}

fn active_test_client(name: &str) -> (AsyncClient<Counter, Counter>) {
    let c = open_test_client(name);
    let ac = AsyncClient::new(c, Counter::default(), Counter::default()).unwrap();
    ac
}

#[test]
fn client_cback_has_proper_default_callbacks() {
    // defaults shouldn't care about these params
    let wc = unsafe { Client::from_raw(ptr::null_mut()) };
    let ps = unsafe { ProcessScope::from_raw(0, ptr::null_mut()) };
    let mut h = ();

    // check each callbacks
    assert_eq!(h.thread_init(&wc), ());
    assert_eq!(h.shutdown(client_status::ClientStatus::empty(), "mock"), ());
    assert_eq!(h.process(&wc, &ps), JackControl::Continue);
    assert_eq!(h.freewheel(&wc, true), ());
    assert_eq!(h.freewheel(&wc, false), ());
    assert_eq!(h.buffer_size(&wc, 0), JackControl::Continue);
    assert_eq!(h.sample_rate(&wc, 0), JackControl::Continue);
    assert_eq!(h.client_registration(&wc, "mock", true), ());
    assert_eq!(h.client_registration(&wc, "mock", false), ());
    assert_eq!(h.port_registration(&wc, 0, true), ());
    assert_eq!(h.port_registration(&wc, 0, false), ());
    assert_eq!(h.port_rename(&wc, 0, "old_mock", "new_mock"),
               JackControl::Continue);
    assert_eq!(h.ports_connected(&wc, 0, 1, true), ());
    assert_eq!(h.ports_connected(&wc, 2, 3, false), ());
    assert_eq!(h.graph_reorder(&wc), JackControl::Continue);
    assert_eq!(h.xrun(&wc), JackControl::Continue);
    assert_eq!(h.latency(&wc, LatencyType::Capture), ());
    assert_eq!(h.latency(&wc, LatencyType::Playback), ());

    mem::forget(wc);
    mem::forget(ps);
}

#[test]
fn client_cback_calls_thread_init() {
    let ac = active_test_client("client_cback_calls_thread_init");
    let counter = ac.deactivate().unwrap().1;
    // IDK why this isn't 1, even with a single thread.
    assert!(*counter.thread_init_count.lock().unwrap() > 0);
}

#[test]
fn client_cback_calls_process() {
    let ac = active_test_client("client_cback_calls_process");
    let counter = ac.deactivate().unwrap().2;
    assert!(counter.frames_processed > 0);
}

#[test]
fn client_cback_calls_buffer_size() {
    let ac = active_test_client("client_cback_calls_process");
    let initial = ac.buffer_size();
    let second = initial / 2;
    let third = second / 2;
    ac.set_buffer_size(second).unwrap();
    ac.set_buffer_size(third).unwrap();
    ac.set_buffer_size(initial).unwrap();
    let counter = ac.deactivate().unwrap().1;
    let mut history_iter = counter.buffer_size_change_history.iter().cloned();
    assert_eq!(history_iter.find(|&s| s == initial), Some(initial));
    assert_eq!(history_iter.find(|&s| s == second), Some(second));
    assert_eq!(history_iter.find(|&s| s == third), Some(third));
    assert_eq!(history_iter.find(|&s| s == initial), Some(initial));
}

#[test]
fn client_cback_calls_after_client_registered() {
    let ac = active_test_client("client_cback_cacr");
    let _other_client = open_test_client("client_cback_cacr_other");
    let counter = ac.deactivate().unwrap().1;
    assert!(counter.registered_client_history
        .contains(&"client_cback_cacr_other".to_string()));
    assert!(!counter.unregistered_client_history
        .contains(&"client_cback_cacr_other".to_string()));
}

#[test]
fn client_cback_calls_after_client_unregistered() {
    let ac = active_test_client("client_cback_cacu");
    let other_client = open_test_client("client_cback_cacu_other");
    drop(other_client);
    let counter = ac.deactivate().unwrap().1;
    assert!(counter.registered_client_history
        .contains(&"client_cback_cacu_other".to_string()));
    assert!(counter.unregistered_client_history
        .contains(&"client_cback_cacu_other".to_string()));
}

#[test]
fn client_cback_reports_xruns() {
    let c = open_test_client("client_cback_reports_xruns");
    let mut counter = Counter::default();
    counter.induce_xruns = true;
    let ac = AsyncClient::new(c, Counter::default(), counter).unwrap();
    let counter = ac.deactivate().unwrap().1;
    assert!(counter.xruns_count > 0, "No xruns encountered.");
}

#[test]
fn client_cback_calls_port_registered() {
    let ac = active_test_client("client_cback_cpr");
    let _pa = ac.register_port("pa", AudioInSpec::default()).unwrap();
    let _pb = ac.register_port("pb", AudioInSpec::default()).unwrap();
    let counter = ac.deactivate().unwrap().1;
    assert_eq!(counter.port_register_history.len(),
               2,
               "Did not detect port registrations.");
    assert!(counter.port_unregister_history.is_empty(),
            "Detected false port deregistrations.");
}

#[test]
fn client_cback_calls_port_unregistered() {
    let ac = active_test_client("client_cback_cpr");
    let _pa = ac.register_port("pa", AudioInSpec::default()).unwrap();
    let _pb = ac.register_port("pb", AudioInSpec::default()).unwrap();
    _pa.unregister().unwrap();
    _pb.unregister().unwrap();
    let counter = ac.deactivate().unwrap().1;
    assert!(counter.port_register_history.len() >= 2,
            "Did not detect port registrations.");
    assert!(counter.port_unregister_history.len() >= 2,
            "Did not detect port deregistrations.");
}
