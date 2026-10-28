// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

#![allow(dead_code)]
use carplay_iap2::{
    transport::{iphone_car_play_configuration::*, iphone_usb_host::*, ncm_function_discovery::*},
    DuplexStream, Receive, Result,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};
#[derive(Default)]
struct Queue {
    bytes: VecDeque<u8>,
    closed: bool,
}
pub struct Memory {
    input: Arc<(Mutex<Queue>, Condvar)>,
    output: Arc<(Mutex<Queue>, Condvar)>,
    pub writes: Mutex<Vec<Vec<u8>>>,
}
pub fn memory_pair() -> (Arc<Memory>, Arc<Memory>) {
    let a = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
    let b = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
    (
        Arc::new(Memory {
            input: a.clone(),
            output: b.clone(),
            writes: Mutex::default(),
        }),
        Arc::new(Memory {
            input: b,
            output: a,
            writes: Mutex::default(),
        }),
    )
}
impl DuplexStream for Memory {
    fn send(&self, b: &[u8]) -> Result<()> {
        let mut q = self.output.0.lock().unwrap();
        if q.closed {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        q.bytes.extend(b);
        self.writes.lock().unwrap().push(b.to_vec());
        self.output.1.notify_all();
        Ok(())
    }
    fn recv(&self, max: usize, timeout: Duration) -> Result<Receive> {
        let q = self.input.0.lock().unwrap();
        let mut q = self
            .input
            .1
            .wait_timeout_while(q, timeout, |q| q.bytes.is_empty() && !q.closed)
            .unwrap()
            .0;
        if !q.bytes.is_empty() {
            let n = max.min(q.bytes.len());
            Ok(Receive::Data(q.bytes.drain(..n).collect()))
        } else if q.closed {
            Ok(Receive::Eof)
        } else {
            Ok(Receive::Timeout)
        }
    }
    fn close(&self) -> Result<()> {
        for q in [&self.input, &self.output] {
            q.0.lock().unwrap().closed = true;
            q.1.notify_all();
        }
        Ok(())
    }
}
pub fn read_exact(stream: &dyn DuplexStream, n: usize) -> Vec<u8> {
    let mut out = vec![];
    while out.len() < n {
        match stream
            .recv(n - out.len(), Duration::from_secs(120))
            .unwrap()
        {
            Receive::Data(b) => out.extend(b),
            other => panic!("Expected data, got {other:?}"),
        }
    }
    out
}
pub fn hex(s: &str) -> Vec<u8> {
    let s = s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
pub fn endpoint(address: u8, packet: u16) -> UsbEndpoint {
    UsbEndpoint {
        address,
        attributes: 2,
        max_packet_size: packet,
        interval: 0,
    }
}
pub fn function(packet: u16) -> NcmFunction {
    let interface = |number, class| UsbInterface {
        number,
        alternate_setting: 1,
        class,
        subclass: 0,
        protocol: 0,
        endpoints: vec![],
        extra: vec![],
    };
    NcmFunction {
        control: interface(2, 2),
        data: interface(3, 0xa),
        bulk_in: endpoint(0x85, packet),
        bulk_out: endpoint(6, packet),
        status_in: None,
    }
}
type ControlSetup = (u8, u8, u16, u16, u16);
#[derive(Default)]
pub struct FakeUsb {
    pub reads: Mutex<VecDeque<Vec<u8>>>,
    pub writes: Mutex<Vec<(Vec<u8>, Duration)>>,
    pub results: Mutex<VecDeque<isize>>,
    pub controls: Mutex<Vec<ControlSetup>>,
    pub control_replies: Mutex<VecDeque<Vec<u8>>>,
    pub claims: Mutex<Vec<u8>>,
    pub released: Mutex<Vec<u8>>,
    pub closed: AtomicBool,
    pub raw: Vec<u8>,
}
impl UsbHostDevice for FakeUsb {
    fn raw_descriptors(&self) -> UsbResult<Vec<u8>> {
        Ok(self.raw.clone())
    }
    fn active_configuration(&self) -> UsbResult<Option<u8>> {
        Ok(Some(6))
    }
    fn set_configuration(&self, _: u8) -> UsbResult<()> {
        Ok(())
    }
    fn detach_kernel_driver(&self, _: u8) -> UsbResult<()> {
        Ok(())
    }
    fn claim_interface(&self, i: u8) -> UsbResult<()> {
        self.claims.lock().unwrap().push(i);
        Ok(())
    }
    fn release_interface(&self, i: u8) -> UsbResult<()> {
        self.released.lock().unwrap().push(i);
        Ok(())
    }
    fn set_alternate_setting(&self, _: u8, _: u8) -> UsbResult<()> {
        Ok(())
    }
    fn control_in(&self, t: u8, r: u8, v: u16, i: u16, n: u16, _: Duration) -> UsbResult<Vec<u8>> {
        self.controls.lock().unwrap().push((t, r, v, i, n));
        Ok(self
            .control_replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_default())
    }
    fn control_out(&self, _: u8, _: u8, _: u16, _: u16, b: &[u8], _: Duration) -> UsbResult<usize> {
        Ok(b.len())
    }
    fn bulk_out(&self, _: u8, b: &[u8], t: Duration) -> UsbResult<usize> {
        self.writes.lock().unwrap().push((b.to_vec(), t));
        match self.results.lock().unwrap().pop_front() {
            Some(-1) => Err(UsbError::Timeout),
            Some(n) => Ok(n as usize),
            None => Ok(b.len()),
        }
    }
    fn bulk_in(&self, _: u8, _: usize, _: Duration) -> UsbResult<Receive> {
        Ok(self
            .reads
            .lock()
            .unwrap()
            .pop_front()
            .map_or(Receive::Timeout, Receive::Data))
    }
    fn interrupt_in(&self, _: u8, _: usize, _: Duration) -> UsbResult<Receive> {
        Ok(Receive::Timeout)
    }
    fn clear_halt(&self, _: u8) -> UsbResult<()> {
        Ok(())
    }
    fn link_speed_mbps(&self) -> f64 {
        480.
    }
    fn close(&self) -> UsbResult<()> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}

impl carplay_iap2::transport::iap2_usb_mux_host::UsbMuxPipe for Memory {
    fn send_with_timeout(&self, data: &[u8], _: Duration) -> Result<()> {
        self.send(data)
    }
}
