//! hidraw transport: find the vendor config interface and run request/response transactions.

use crate::protocol::{self, Packet, PACKET_LEN};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const VENDOR_ID: u16 = 0x35bb;
/// 2.4 GHz receiver and wired (USB-C) product IDs of the G3M PRO updated version.
pub const RECEIVER_PID: u16 = 0xd3fb;
pub const WIRED_PID: u16 = 0xd3fc;
pub const PRODUCT_IDS: [(u16, &str); 2] = [(RECEIVER_PID, "2.4G receiver"), (WIRED_PID, "wired")];
/// Vendor usage page that carries the 0x08 config report.
const CONFIG_USAGE_PAGE: u16 = 0xff02;
const HIDRAW_SYSFS: &str = "/sys/class/hidraw";

/// The vendor library uses 2 tries x 200 ms but retries whole operations at a higher level.
/// Over 2.4G, replies are occasionally late or lost while the mouse drifts in and out of sleep
/// (seen live 2026-10-21), so retry more. Reads and writes are idempotent, so resending is safe.
const TRIES: u32 = 4;
const TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub node: PathBuf,
    pub product_id: u16,
    pub kind: &'static str,
    /// USB bcdDevice of the receiver or cable, shown by the vendor as the receiver firmware (FUN_1400fa0c0).
    pub receiver_version: Option<u16>,
}

impl DeviceInfo {
    /// The 2.4 GHz receiver, the only connection the vendor pairs through (RepairDongle, ui-features §6.2).
    pub fn is_receiver(&self) -> bool {
        self.product_id == RECEIVER_PID
    }
}

/// Lists hidraw nodes that belong to a supported mouse and expose the config report.
pub fn find_devices() -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(HIDRAW_SYSFS) else { return out };
    for e in entries.flatten() {
        let sys = e.path();
        let Ok(uevent) = fs::read_to_string(sys.join("device/uevent")) else { continue };
        let Some((vid, pid)) = parse_hid_id(&uevent) else { continue };
        if vid != VENDOR_ID {
            continue;
        }
        let Some(&(_, kind)) = PRODUCT_IDS.iter().find(|(p, _)| *p == pid) else { continue };
        let Ok(desc) = fs::read(sys.join("device/report_descriptor")) else { continue };
        if !report_ids_on_page(&desc, CONFIG_USAGE_PAGE).contains(&protocol::REPORT_ID) {
            continue;
        }
        let receiver_version = read_bcd_device(&sys);
        out.push(DeviceInfo { node: Path::new("/dev").join(e.file_name()), product_id: pid, kind, receiver_version });
    }
    out.sort_by(|a, b| a.node.cmp(&b.node));
    out
}

/// USB bcdDevice for a hidraw node such as `/dev/hidraw1`.
pub fn receiver_version(node: &Path) -> Option<u16> {
    read_bcd_device(&Path::new(HIDRAW_SYSFS).join(node.file_name()?))
}

/// `<hidraw sysfs dir>/device` is the HID device; its parent is the USB interface, whose parent is the USB device.
fn read_bcd_device(sys: &Path) -> Option<u16> {
    parse_bcd_device(&fs::read_to_string(sys.join("device/../../bcdDevice")).ok()?)
}

/// Parses sysfs `bcdDevice` (`"0102\n"`, hex).
pub fn parse_bcd_device(s: &str) -> Option<u16> {
    let s = s.trim();
    if s.is_empty() || s.len() > 4 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u16::from_str_radix(s, 16).ok()
}

/// Parses `HID_ID=0003:000035BB:0000D3FB` from a hidraw uevent.
pub fn parse_hid_id(uevent: &str) -> Option<(u16, u16)> {
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':');
    let _bus = parts.next()?;
    let vid = u32::from_str_radix(parts.next()?, 16).ok()?;
    let pid = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((u16::try_from(vid).ok()?, u16::try_from(pid).ok()?))
}

/// Walks HID short items and returns the report IDs declared while `page` is the usage page.
pub fn report_ids_on_page(desc: &[u8], page: u16) -> Vec<u8> {
    let mut ids = Vec::new();
    let mut cur_page: u32 = 0;
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xfe {
            // long item: 0xfe, size, tag, data
            let size = *desc.get(i + 1).unwrap_or(&0) as usize;
            i += 3 + size;
            continue;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let Some(data) = desc.get(i + 1..i + 1 + size) else { break };
        let value = data.iter().rev().fold(0u32, |acc, b| (acc << 8) | *b as u32);
        match prefix & 0xfc {
            0x04 => cur_page = value,                                    // Usage Page
            0x84 if cur_page == page as u32 => ids.push(value as u8),    // Report ID
            _ => {}
        }
        i += 1 + size;
    }
    ids
}

/// Set to print every request/response packet on stderr.
const DEBUG_ENV: &str = "HECATECTL_DEBUG";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// How long a locked operation waits for another process's operation to finish.
const LOCK_WAIT: Duration = Duration::from_secs(10);
const LOCK_POLL: Duration = Duration::from_millis(20);

/// Re-entrant lock bookkeeping for one Device: only the outermost lock takes the OS lock and only the
/// matching outermost unlock releases it.
#[derive(Debug, Default)]
struct LockDepth(u32);

impl LockDepth {
    /// Whether the next `enter` must take the OS lock first.
    fn needs_os_lock(&self) -> bool {
        self.0 == 0
    }

    fn enter(&mut self) {
        self.0 += 1;
    }

    /// Leaves one level. True when that was the outermost one, so the OS lock must be released.
    /// An unmatched leave is ignored.
    fn leave(&mut self) -> bool {
        match self.0 {
            0 => false,
            n => {
                self.0 = n - 1;
                n == 1
            }
        }
    }
}

/// `flock(LOCK_EX | LOCK_NB)`, retried until `wait` has passed.
fn lock_file(file: &File, wait: Duration, debug: bool) -> Result<(), String> {
    let deadline = Instant::now() + wait;
    let mut announced = false;
    loop {
        // SAFETY: the descriptor stays open for the duration of the call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        match e.raw_os_error() {
            Some(libc::EINTR) => continue,
            Some(libc::EWOULDBLOCK) => {}
            _ => return Err(format!("lock: {e}")),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "device busy: another hecatectl or hecate-gui operation is running (waited {} s)",
                wait.as_secs()
            ));
        }
        if debug && !announced {
            eprintln!("lock busy, waiting");
            announced = true;
        }
        std::thread::sleep(LOCK_POLL);
    }
}

fn unlock_file(file: &File) {
    // SAFETY: as above. Unlocking cannot fail in a way we could act on; closing the fd releases it anyway.
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
}

pub struct Device {
    file: File,
    debug: bool,
    lock: LockDepth,
    lock_wait: Duration,
}

impl Device {
    pub fn open(node: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(node)
            .map_err(|e| match e.kind() {
                ErrorKind::PermissionDenied => format!(
                    "{}: permission denied (install udev/70-hecatectl.rules, then replug the receiver)",
                    node.display()
                ),
                _ => format!("{}: {e}", node.display()),
            })?;
        Ok(Self { file, debug: std::env::var_os(DEBUG_ENV).is_some(), lock: LockDepth::default(), lock_wait: LOCK_WAIT })
    }

    /// Exclusive advisory lock on the hidraw node (`flock`), so operations of separate processes do not
    /// interleave and take each other's replies. Re-entrant: nested calls only count. Every `lock` needs
    /// one `unlock`. Waits up to 10 s for another process, then fails with "device busy".
    pub fn lock(&mut self) -> Result<(), String> {
        if self.lock.needs_os_lock() {
            lock_file(&self.file, self.lock_wait, self.debug)?;
        }
        self.lock.enter();
        Ok(())
    }

    pub fn unlock(&mut self) {
        if self.lock.leave() {
            unlock_file(&self.file);
        }
    }

    /// Whether this Device is inside a locked operation.
    pub fn is_locked(&self) -> bool {
        !self.lock.needs_os_lock()
    }

    #[cfg(test)]
    pub(crate) fn set_lock_wait(&mut self, wait: Duration) {
        self.lock_wait = wait;
    }

    /// Sends `req` and waits for the matching response, like the vendor library does.
    pub fn transact(&mut self, req: &Packet) -> Result<Packet, String> {
        self.transact_with(req, TRIES, TIMEOUT)
    }

    /// `transact` with an explicit retry count and per-try timeout, under the device lock.
    pub fn transact_with(&mut self, req: &Packet, tries: u32, timeout: Duration) -> Result<Packet, String> {
        self.lock()?;
        let result = self.transact_locked(req, tries, timeout);
        self.unlock();
        result
    }

    fn transact_locked(&mut self, req: &Packet, tries: u32, timeout: Duration) -> Result<Packet, String> {
        for _ in 0..tries {
            self.drain();
            if self.debug {
                eprintln!("tx {}", hex(req));
            }
            self.file.write_all(req).map_err(|e| format!("write: {e}"))?;
            if let Some(resp) = self.wait_for(req, timeout)? {
                if self.debug {
                    eprintln!("rx {}", hex(&resp));
                }
                return Ok(resp);
            }
        }
        Err(format!("no response to command 0x{:02x} (is the mouse awake and paired?)", req[1]))
    }

    fn wait_for(&mut self, req: &Packet, timeout: Duration) -> Result<Option<Packet>, String> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 64];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() || !self.poll_readable(left)? {
                return Ok(None);
            }
            match self.file.read(&mut buf) {
                Ok(n) if protocol::is_response_to(req, &buf[..n]) => {
                    let mut p = [0u8; PACKET_LEN];
                    p.copy_from_slice(&buf[..PACKET_LEN]);
                    return Ok(Some(p));
                }
                Ok(n) => {
                    // mouse motion, unsolicited 08 0a / 08 b8 notices, or other reports
                    if self.debug && buf[0] == protocol::REPORT_ID {
                        eprintln!("rx (skipped) {}", hex(&buf[..n]));
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("read: {e}")),
            }
        }
    }

    /// Discards queued input reports so stale answers cannot match.
    fn drain(&mut self) {
        let mut buf = [0u8; 64];
        while self.file.read(&mut buf).is_ok() {}
    }

    fn poll_readable(&self, timeout: Duration) -> Result<bool, String> {
        let mut pfd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ms = timeout.as_millis().clamp(1, i32::MAX as u128) as i32;
        // SAFETY: pfd is a valid pollfd for the duration of the call.
        let r = unsafe { libc::poll(&mut pfd, 1, ms) };
        if r < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(format!("poll: {e}"));
        }
        Ok(r > 0 && pfd.revents & libc::POLLIN != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Report descriptor of interface 1 of the 35BB:D3FB receiver (from sysfs).
    const RECEIVER_IF1: &str = "05010902a10185010901a1000509190129051500250195057501810295017503810105010930093116018026ff7f75109502810609381581257f750895018106050c0a380295018106c0c00603ff0900a10185020900150026ff00750895078102c0050c0901a101850526140519002a1405751095018100c005010980a1018503198129832501950375018102950175058101c00602ff0902a101850826ff00750895100902810009029100c00604ff0902a101850609029507b102c0";
    // Interface 0 (keyboard), which has no config report.
    const RECEIVER_IF0: &str = "05010906a101050719e029e715002501750195088102750895018101050719002aff0026ff00750895068100c0";

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn finds_config_report_on_vendor_page() {
        assert_eq!(report_ids_on_page(&hex(RECEIVER_IF1), 0xff02), vec![0x08]);
        assert_eq!(report_ids_on_page(&hex(RECEIVER_IF1), 0xff04), vec![0x06]);
    }

    #[test]
    fn keyboard_interface_has_no_config_report() {
        assert!(report_ids_on_page(&hex(RECEIVER_IF0), 0xff02).is_empty());
    }

    #[test]
    fn empty_and_truncated_descriptors() {
        assert!(report_ids_on_page(&[], 0xff02).is_empty());
        // usage page item whose data runs past the end
        assert!(report_ids_on_page(&[0x06, 0x02], 0xff02).is_empty());
    }

    #[test]
    fn parses_hid_id() {
        let ue = "DRIVER=hid-generic\nHID_ID=0003:000035BB:0000D3FB\nHID_NAME=HECATE\n";
        assert_eq!(parse_hid_id(ue), Some((0x35bb, 0xd3fb)));
    }

    #[test]
    fn parses_bcd_device() {
        assert_eq!(parse_bcd_device("0102\n"), Some(0x0102)); // live receiver
        assert_eq!(parse_bcd_device("0000"), Some(0));
        assert_eq!(parse_bcd_device("ffff"), Some(0xffff));
        assert_eq!(parse_bcd_device("1"), Some(1));
        assert_eq!(parse_bcd_device(""), None);
        assert_eq!(parse_bcd_device("\n"), None);
        assert_eq!(parse_bcd_device("10000"), None);
        assert_eq!(parse_bcd_device("+102"), None);
        assert_eq!(parse_bcd_device("01g2"), None);
    }

    #[test]
    fn rejects_bad_hid_id() {
        assert_eq!(parse_hid_id(""), None);
        assert_eq!(parse_hid_id("HID_ID=0003:zz:0000D3FB"), None);
        assert_eq!(parse_hid_id("HID_ID=0003:0001000000:0000D3FB"), None);
    }

    #[test]
    fn receiver_is_identified_by_product_id() {
        let info = |product_id| DeviceInfo {
            node: PathBuf::from("/dev/hidraw1"),
            product_id,
            kind: "",
            receiver_version: None,
        };
        assert!(info(RECEIVER_PID).is_receiver());
        assert!(!info(WIRED_PID).is_receiver());
        assert!(!info(0).is_receiver());
    }

    #[test]
    fn lock_depth_counts_nesting() {
        let mut d = LockDepth::default();
        assert!(d.needs_os_lock());
        d.enter();
        assert!(!d.needs_os_lock());
        d.enter(); // nested operation
        assert!(!d.leave(), "inner unlock keeps the OS lock");
        assert!(!d.needs_os_lock());
        assert!(d.leave(), "outermost unlock releases it");
        assert!(d.needs_os_lock());
        assert!(!d.leave(), "unmatched unlock is ignored");
        assert!(d.needs_os_lock());
    }

    /// An empty regular file in the temp dir; flock works on it like on the hidraw node.
    fn temp_file(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("hecatectl-lock-{tag}-{}-{nanos}", std::process::id()));
        fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn second_descriptor_cannot_lock_while_the_first_holds_it() {
        let path = temp_file("fd");
        let (a, b) = (File::open(&path).unwrap(), File::open(&path).unwrap());
        assert_eq!(lock_file(&a, Duration::ZERO, false), Ok(()));
        let started = Instant::now();
        let err = lock_file(&b, Duration::from_millis(60), false).unwrap_err();
        assert!(err.starts_with("device busy"), "{err}");
        assert!(started.elapsed() >= Duration::from_millis(60), "waited the bounded time");
        unlock_file(&a);
        assert_eq!(lock_file(&b, Duration::ZERO, false), Ok(()));
        unlock_file(&b);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn device_lock_is_reentrant_and_released_by_the_outermost_unlock() {
        let path = temp_file("dev");
        let (mut a, mut b) = (Device::open(&path).unwrap(), Device::open(&path).unwrap());
        b.lock_wait = Duration::from_millis(30);
        assert!(!a.is_locked());
        a.lock().unwrap();
        a.lock().unwrap(); // a composite operation calling another one
        assert!(a.is_locked() && !b.is_locked());
        assert!(b.lock().is_err());
        a.unlock();
        assert!(b.lock().is_err(), "inner unlock must not release the lock");
        a.unlock();
        assert!(!a.is_locked());
        assert_eq!(b.lock(), Ok(()));
        a.lock_wait = Duration::from_millis(30);
        assert!(a.lock().is_err(), "now the other side holds it");
        b.unlock();
        assert_eq!(a.lock(), Ok(()));
        a.unlock();
        fs::remove_file(&path).unwrap();
    }
}
