//! Minimal Linux evdev access with raw ioctls and raw event bytes.
//! The evdev crate builds `libc::input_event`, whose layout differs on
//! 32-bit musl with 64-bit time (Alpine armv7): here nothing depends on it.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

pub const EV_KEY: u16 = 0x01;
pub const EV_FF: u16 = 0x15;
pub const FF_RUMBLE: u16 = 0x50;
pub const KEY_VOLUMEDOWN: u16 = 114;
pub const KEY_VOLUMEUP: u16 = 115;
pub const KEY_POWER: u16 = 116;

const LONG: usize = std::mem::size_of::<libc::c_long>();
const EVENT_SIZE: usize = 2 * LONG + 8;
const PTR: usize = std::mem::size_of::<usize>();
// struct ff_effect: 14 header bytes, union aligned to a pointer, the largest
// member (ff_periodic_effect) ends with a pointer
const FF_EFFECT_SIZE: usize = 16 + (24usize.div_ceil(PTR) * PTR) + PTR;

const fn ioc(dir: u32, nr: u32, size: usize) -> libc::c_ulong {
    ((dir << 30) | ((size as u32) << 16) | (0x45 << 8) | nr) as libc::c_ulong
}

pub struct Event {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

pub struct Device {
    file: File,
}

/// All /dev/input/event* devices we can open.
pub fn enumerate() -> Vec<Device> {
    let Ok(dir) = std::fs::read_dir("/dev/input") else { return vec![] };
    let mut paths: Vec<_> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event")))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_CLOEXEC)
                .open(&p)
                .or_else(|_| File::open(&p))
                .ok()
        })
        .map(|file| Device { file })
        .collect()
}

impl Device {
    fn bits(&self, ev: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 96];
        // EVIOCGBIT(ev, len)
        let req = ioc(2, 0x20 + ev as u32, buf.len());
        let n = unsafe { libc::ioctl(self.file.as_raw_fd(), req as _, buf.as_mut_ptr()) };
        if n < 0 {
            buf.clear();
        }
        buf
    }

    fn has(bits: &[u8], code: u16) -> bool {
        bits.get(code as usize / 8).is_some_and(|b| b & (1 << (code % 8)) != 0)
    }

    pub fn has_key(&self, code: u16) -> bool {
        Self::has(&self.bits(EV_KEY), code)
    }

    pub fn has_rumble(&self) -> bool {
        Self::has(&self.bits(EV_FF), FF_RUMBLE)
    }

    /// Blocking read of the next batch of events.
    pub fn read_events(&mut self) -> std::io::Result<Vec<Event>> {
        let mut buf = [0u8; EVENT_SIZE * 32];
        let n = self.file.read(&mut buf)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        Ok(buf[..n - n % EVENT_SIZE]
            .chunks_exact(EVENT_SIZE)
            .map(|c| {
                let t = 2 * LONG;
                Event {
                    kind: u16::from_ne_bytes([c[t], c[t + 1]]),
                    code: u16::from_ne_bytes([c[t + 2], c[t + 3]]),
                    value: i32::from_ne_bytes([c[t + 4], c[t + 5], c[t + 6], c[t + 7]]),
                }
            })
            .collect())
    }

    /// Uploads a rumble effect and plays it once. Returns false if unsupported.
    pub fn rumble(&mut self, ms: u16) -> bool {
        let mut eff = [0u8; FF_EFFECT_SIZE];
        eff[0..2].copy_from_slice(&FF_RUMBLE.to_ne_bytes());
        eff[2..4].copy_from_slice(&(-1i16).to_ne_bytes());
        eff[10..12].copy_from_slice(&ms.to_ne_bytes()); // replay.length
        eff[16..18].copy_from_slice(&0xffffu16.to_ne_bytes()); // strong
        eff[18..20].copy_from_slice(&0xffffu16.to_ne_bytes()); // weak
        // EVIOCSFF
        let req = ioc(1, 0x80, FF_EFFECT_SIZE);
        if unsafe { libc::ioctl(self.file.as_raw_fd(), req as _, eff.as_mut_ptr()) } < 0 {
            return false;
        }
        let id = i16::from_ne_bytes([eff[2], eff[3]]);
        let mut ev = [0u8; EVENT_SIZE];
        let t = 2 * LONG;
        ev[t..t + 2].copy_from_slice(&EV_FF.to_ne_bytes());
        ev[t + 2..t + 4].copy_from_slice(&(id as u16).to_ne_bytes());
        ev[t + 4..t + 8].copy_from_slice(&1i32.to_ne_bytes());
        let ok = self.file.write_all(&ev).is_ok();
        std::thread::sleep(std::time::Duration::from_millis(ms as u64 + 50));
        // EVIOCRMFF
        unsafe { libc::ioctl(self.file.as_raw_fd(), ioc(1, 0x81, std::mem::size_of::<libc::c_int>()) as _, id as libc::c_int) };
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout() {
        #[cfg(target_pointer_width = "64")]
        assert_eq!((EVENT_SIZE, FF_EFFECT_SIZE), (24, 48));
        #[cfg(target_pointer_width = "32")]
        assert_eq!((EVENT_SIZE, FF_EFFECT_SIZE), (16, 44));
        assert_eq!(ioc(2, 0x20 + 1, 96), 0x80604521);
        assert_eq!(ioc(1, 0x80, 48), 0x40304580);
    }
}
