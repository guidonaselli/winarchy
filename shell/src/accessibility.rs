use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use accesskit::{Action, ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use accesskit_windows::Adapter;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_ACTION: u32 = WM_APP + 40;
static GENERATION: AtomicUsize = AtomicUsize::new(1);

struct NodeRegistry {
    next: u64,
    live: HashMap<String, u64>,
}

impl NodeRegistry {
    fn new() -> Self { Self { next: 1, live: HashMap::new() } }
    fn sync(&mut self, keys: &[String]) -> Vec<u64> {
        self.live.retain(|key, _| keys.contains(key));
        keys.iter().map(|key| {
            *self.live.entry(key.clone()).or_insert_with(|| {
                let id = self.next;
                self.next += 1;
                id
            })
        }).collect()
    }
    fn resolve(&self, id: u64) -> Option<&str> {
        self.live.iter().find_map(|(key, value)| (*value == id).then_some(key.as_str()))
    }
}

struct Handler {
    hwnd: isize,
    generation: usize,
    queue: Arc<Mutex<Vec<ActionRequest>>>,
}

impl ActionHandler for Handler {
    fn do_action(&mut self, request: ActionRequest) {
        if request.target_tree != TreeId::ROOT { return }
        if let Ok(mut queue) = self.queue.lock() {
            queue.push(request);
            unsafe {
                if PostMessageW(HWND(self.hwnd as *mut _), WM_ACTION, WPARAM(self.generation), LPARAM(0)).is_err() {
                    queue.pop();
                }
            }
        }
    }
}

struct Host {
    adapter: Adapter,
    role: Role,
    snapshot: TreeUpdate,
    registry: NodeRegistry,
    generation: usize,
    queue: Arc<Mutex<Vec<ActionRequest>>>,
}

thread_local! { static HOSTS: RefCell<HashMap<isize, Host>> = RefCell::new(HashMap::new()); }

struct Initial(TreeUpdate);
impl ActivationHandler for Initial {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> { Some(self.0.clone()) }
}

pub fn create(hwnd: HWND, label: &str, role: Role) {
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    let queue = Arc::new(Mutex::new(Vec::new()));
    let mut root = Node::new(role);
    root.set_label(label);
    let snapshot = TreeUpdate { nodes: vec![(NodeId(0), root)], tree: Some(TreeInfo::new(NodeId(0))), tree_id: TreeId::ROOT, focus: NodeId(0) };
    let adapter = Adapter::new(accesskit_windows::HWND(hwnd.0), false, Handler { hwnd: hwnd.0 as isize, generation, queue: queue.clone() });
    HOSTS.with(|hosts| { hosts.borrow_mut().insert(hwnd.0 as isize, Host { adapter, role, snapshot, registry: NodeRegistry::new(), generation, queue }); });
}

pub fn destroy(hwnd: HWND) {
    HOSTS.with(|hosts| { hosts.borrow_mut().remove(&(hwnd.0 as isize)); });
}

pub fn get_object(hwnd: HWND, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    let result = HOSTS.with(|hosts| {
        let mut hosts = hosts.try_borrow_mut().ok()?;
        let host = hosts.get_mut(&(hwnd.0 as isize))?;
        let mut initial = Initial(host.snapshot.clone());
        host.adapter.handle_wm_getobject(accesskit_windows::WPARAM(wp.0), accesskit_windows::LPARAM(lp.0), &mut initial)
    });
    result.map(|value| { let value: accesskit_windows::LRESULT = value.into(); LRESULT(value.0) })
}

pub fn publish(hwnd: HWND, label: &str, size: (i32, i32), nodes: Vec<(String, Node)>, focus: Option<&str>) {
    let events = HOSTS.with(|hosts| {
        let mut hosts = hosts.try_borrow_mut().ok()?;
        let host = hosts.get_mut(&(hwnd.0 as isize))?;
        let keys: Vec<String> = nodes.iter().map(|(key, _)| key.clone()).collect();
        let ids = host.registry.sync(&keys);
        let mut root = Node::new(host.role);
        root.set_label(label);
        root.set_bounds(accesskit::Rect::new(0.0, 0.0, size.0.max(0) as f64, size.1.max(0) as f64));
        root.set_children(ids.iter().copied().map(NodeId).collect::<Vec<_>>());
        let focused = focus.and_then(|key| keys.iter().position(|k| k == key)).map(|i| NodeId(ids[i])).unwrap_or(NodeId(0));
        let mut all = vec![(NodeId(0), root)];
        all.extend(nodes.into_iter().zip(ids).map(|((_, node), id)| (NodeId(id), node)));
        let update = TreeUpdate { nodes: all, tree: Some(TreeInfo::new(NodeId(0))), tree_id: TreeId::ROOT, focus: focused };
        if host.snapshot == update { return None }
        host.snapshot = update.clone();
        host.adapter.update_if_active(|| update)
    });
    if let Some(events) = events { events.raise(); }
}

pub fn focus(hwnd: HWND, focused: bool) {
    let events = HOSTS.with(|hosts| hosts.try_borrow_mut().ok()?.get_mut(&(hwnd.0 as isize))?.adapter.update_window_focus_state(focused));
    if let Some(events) = events { events.raise(); }
}

pub fn actions(hwnd: HWND, generation: usize) -> Vec<(String, Action)> {
    HOSTS.with(|hosts| {
        let Ok(mut hosts) = hosts.try_borrow_mut() else { return Vec::new() };
        let Some(host) = hosts.get_mut(&(hwnd.0 as isize)).filter(|host| host.generation == generation) else { return Vec::new() };
        let Ok(mut queue) = host.queue.lock() else { return Vec::new() };
        std::mem::take(&mut *queue).into_iter().filter_map(|request| {
            host.registry.resolve(request.target_node.0).map(|key| (key.to_string(), request.action))
        }).collect()
    })
}

#[cfg(test)]
#[path = "accessibility_tests.rs"]
mod tests;
