// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

mod wired_common;
use carplay_iap2::{
    orchestration::{
        wired_configuration_action::WiredConfigurationAction as Action,
        wired_session_memory::WiredSessionMemory,
    },
    transport::{
        iphone_car_play_configuration::*, iphone_usb_host::*,
        ncm_function_discovery::NcmFunctionDiscovery, ncm_usb_bridge::NcmUsbBridge,
        usb_configuration_selection::*,
    },
};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
    time::Duration,
};
use wired_common::*;
#[test]
fn wired_configuration_decision_table_and_memory() {
    for (found, mode, tried, started, expected) in [
        (true, true, 0, false, Action::Use),
        (true, true, 1, false, Action::Use),
        (true, false, 0, false, Action::RequestTransition),
        (true, false, 1, false, Action::UseWithoutAppleEthernet),
        (false, false, 0, false, Action::RequestTransition),
        (false, false, 1, false, Action::RequestTransition),
        (false, false, 2, false, Action::Reject),
        (true, true, 0, true, Action::RequestTransition),
        (true, true, 1, true, Action::Use),
    ] {
        assert_eq!(Action::choose(found, mode, tried, 2, started), expected);
    }
    assert!(Action::fallback_after_rejection(true, false, false));
    assert!(!Action::fallback_after_rejection(false, false, false));
    assert!(Action::fallback_after_rejection(true, true, true));
    assert!(!Action::fallback_after_rejection(true, true, false));
    assert_eq!(
        [
            Action::Use.diagnostic(),
            Action::UseWithoutAppleEthernet.diagnostic(),
            Action::RequestTransition.diagnostic(),
            Action::Reject.diagnostic()
        ],
        [
            "reuse-descriptors",
            "reuse-without-apple-ethernet",
            "request-transition",
            "reject-missing-configuration"
        ]
    );
    let memory = WiredSessionMemory::default();
    assert!(!memory.started_on("node5"));
    memory.started("node5");
    assert!(memory.started_on("node5"));
    assert!(!memory.started_on("node6"));
    memory.clear();
    assert!(!memory.started_on("node5"));
}
#[test]
fn selection_first_success_and_already_active_do_not_detach() {
    assert!(UsbConfigurationSelection::select_with(
        6,
        &[0, 1],
        || Ok(()),
        || Ok(Some(6)),
        |_| panic!(),
        || panic!()
    )
    .unwrap()
    .is_none());
    let current = UsbConfigurationSelection::select_with(
        6,
        &[0, 3],
        || Err(UsbError::InterfaceBusy(0)),
        || Ok(Some(6)),
        |_| panic!(),
        || panic!(),
    )
    .unwrap()
    .unwrap();
    assert!(!current.wrong_configuration());
    assert!(current.detached.is_empty());
}
#[test]
fn selection_sorted_detach_then_retry_exact_diagnostic() {
    let active = Cell::new(2);
    let attempts = Cell::new(0);
    let reopened = Cell::new(false);
    let calls = RefCell::new(vec![]);
    let outcome = UsbConfigurationSelection::select_with(
        6,
        &[2, 0, 1, 0, 3],
        || {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 2 && reopened.get() {
                active.set(6);
                Ok(())
            } else {
                Err(UsbError::InterfaceBusy(0))
            }
        },
        || Ok(Some(active.get())),
        |i| {
            assert!(!reopened.get());
            calls.borrow_mut().push(i);
            if i == 3 {
                Err(UsbError::InterfaceBusy(i))
            } else {
                Ok(())
            }
        },
        || {
            reopened.set(true);
            Ok(())
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(*calls.borrow(), [0, 1, 2, 3]);
    assert_eq!(
        outcome.diagnostic(),
        "USB configuration select target=6 ok=true before=2 detached=0,1,2 after=6"
    );
    assert!(!outcome.wrong_configuration());
}
#[test]
fn selection_stuck_unreadable_and_unplug() {
    let stuck = UsbConfigurationSelection::select_with(
        6,
        &[0],
        || Err(UsbError::InterfaceBusy(0)),
        || Ok(Some(2)),
        |_| Err(UsbError::InterfaceBusy(0)),
        || panic!(),
    )
    .unwrap()
    .unwrap();
    assert!(stuck.wrong_configuration());
    assert_eq!(
        stuck.diagnostic(),
        "USB configuration select target=6 ok=false before=2 detached=none after=2"
    );
    let unknown = UsbConfigurationSelection::select_with(
        6,
        &[0],
        || Err(UsbError::InterfaceBusy(0)),
        || Ok(None),
        |_| Ok(()),
        || Ok(()),
    )
    .unwrap()
    .unwrap();
    assert!(!unknown.wrong_configuration());
    assert!(matches!(
        UsbConfigurationSelection::select_with(
            6,
            &[],
            || Err(UsbError::DeviceGone),
            || panic!(),
            |_| panic!(),
            || panic!()
        ),
        Err(UsbError::DeviceGone)
    ));
}
#[test]
fn mode_diagnostics_and_request_constants() {
    assert_eq!(IphoneUsbHost::usb_mode_diagnostic(4,&[3,3,3,0],Some("15.04"),&["1[0/0:6.1.1x3]".into()]),"CarPlay USB mode request rejected; current mode=3:3:3:0 usbVersion=15.04 configurations=[1[0/0:6.1.1x3]]");
    assert!(IphoneUsbHost::usb_mode_diagnostic(-1, &[0; 4], None, &[])
        .contains("current mode=unreadable(-1) usbVersion=unknown"));
    assert!(
        IphoneUsbHost::usb_mode_diagnostic(3, &[1, 2, 3, 0], Some("7.01"), &[])
            .contains("current mode=1:2:3 (short:3) usbVersion=7.01")
    );
    let usb = FakeUsb::default();
    usb.control_replies.lock().unwrap().push_back(vec![0]);
    let descriptors = UsbDescriptors {
        device: vec![],
        configurations: vec![],
    };
    assert!(matches!(
        IphoneUsbHost::request_car_play_mode(&usb, &descriptors).unwrap(),
        CarPlayModeOutcome::ReenumerationRequested
    ));
    assert_eq!(*usb.controls.lock().unwrap(), [(0xc0, 0x52, 0, 4, 1)]);
    usb.control_replies
        .lock()
        .unwrap()
        .extend([vec![], vec![1, 2, 3]]);
    assert!(matches!(
        IphoneUsbHost::request_car_play_mode(&usb, &descriptors).unwrap(),
        CarPlayModeOutcome::Rejected { transferred: 0, .. }
    ));
    assert_eq!(
        usb.controls.lock().unwrap().last(),
        Some(&(0xc0, 0x45, 0, 0, 4))
    );
}
fn interface(number: u8, class: u8, subclass: u8, protocol: u8) -> UsbInterface {
    UsbInterface {
        number,
        alternate_setting: 0,
        class,
        subclass,
        protocol,
        endpoints: vec![],
        extra: vec![],
    }
}
#[test]
fn configuration_discovery_prefers_apple_ethernet_then_ncm_and_endpoints() {
    let mut mux = interface(1, 255, 254, 2);
    mux.endpoints = vec![
        endpoint(3, 512),
        endpoint(0x84, 512),
        endpoint(4, 512),
        endpoint(0x85, 512),
    ];
    let c = UsbConfiguration {
        value: 5,
        interfaces: vec![mux.clone(), interface(2, 2, 13, 0)],
    };
    let mut cp = c.clone();
    cp.value = 6;
    cp.interfaces.push(interface(4, 255, 253, 1));
    let configs = [c.clone(), cp.clone()];
    assert_eq!(IphoneCarPlayConfiguration::find(&configs), Some(&cp));
    assert!(!IphoneCarPlayConfiguration::is_car_play_mode(&c));
    assert_eq!(
        IphoneCarPlayConfiguration::find(std::slice::from_ref(&c)),
        Some(&c)
    );
    assert_eq!(
        IphoneCarPlayConfiguration::describe(&c),
        "1/0:ff.fe.2x4,2/0:2.d.0x0"
    );
    let (out, input) = IphoneCarPlayConfiguration::usb_mux_endpoints(&mux).unwrap();
    assert_eq!((out.address, input.address), (4, 0x85));
    mux.endpoints = vec![endpoint(3, 64), endpoint(0x84, 64)];
    assert!(IphoneCarPlayConfiguration::usb_mux_endpoints(&mux).is_some());
    mux.endpoints.push(endpoint(0x86, 64));
    assert!(IphoneCarPlayConfiguration::usb_mux_endpoints(&mux).is_none());
}
#[test]
fn descriptor_parser_keeps_alternates_and_class_descriptors() {
    let raw=hex("1201000200000040ac0501120100000000010902200001060080000904020001020d00000524060f070705830340000109040301020a0000000705850200020007050602000200");
    let parsed = UsbDescriptors::parse(&raw).unwrap();
    assert_eq!(parsed.usb_version().as_deref(), Some("0.01"));
    let c = &parsed.configurations[0];
    let ncm = NcmFunctionDiscovery::find(c).unwrap();
    assert_eq!(
        (
            ncm.control.number,
            ncm.data.number,
            ncm.data.alternate_setting
        ),
        (2, 3, 1)
    );
    assert_eq!(ncm.status_in.unwrap().address, 0x83);
    assert_eq!(ncm.control.extra, hex("0524060f07"));
    assert!(UsbDescriptors::parse(&[0, 1]).is_err());
    assert!(UsbDescriptors::parse(&[9, 4, 0]).is_err());
}
#[test]
fn ncm_descriptor_mac_takes_priority_and_fallback_is_stable() {
    let usb = Arc::new(FakeUsb::default());
    let mut f = function(512);
    f.control.extra = vec![4, 0x24, 0x0f, 7];
    let mut descriptor = vec![26, 3];
    for c in "020000000002".encode_utf16() {
        descriptor.extend_from_slice(&c.to_le_bytes());
    }
    usb.control_replies.lock().unwrap().push_back(descriptor);
    let b = NcmUsbBridge::open(usb.clone(), f, [1; 6], Arc::new(|_| {})).unwrap();
    assert_eq!(b.host_mac(), [2, 0, 0, 0, 0, 2]);
    assert_eq!(
        usb.controls.lock().unwrap()[0],
        (0x80, 6, 0x307, 0x409, 256)
    );
    b.close().unwrap();
    let b = NcmUsbBridge::open(usb, function(512), [1; 6], Arc::new(|_| {})).unwrap();
    assert_eq!(b.host_mac(), [1; 6]);
    b.send(SMALL_FRAME, Duration::from_millis(5)).unwrap();
}
const SMALL_FRAME: &[u8] = &[0x33, 0x33, 0, 0, 0, 1, 0x86, 0xdd];

#[test]
fn apple_ethernet_claim_is_deduplicated_across_alternate_settings() {
    let usb = FakeUsb::default();
    let mut second = interface(4, 255, 253, 1);
    second.alternate_setting = 1;
    let config = UsbConfiguration {
        value: 6,
        interfaces: vec![interface(4, 255, 253, 1), second],
    };
    let descriptors = UsbDescriptors {
        device: vec![],
        configurations: vec![config.clone()],
    };
    IphoneUsbHost::select_configuration(&usb, &descriptors, &config).unwrap();
    assert_eq!(*usb.claims.lock().unwrap(), [4]);
}

#[test]
fn mode_diagnostic_retains_a_successful_zero_byte_mode_read() {
    let usb = FakeUsb::default();
    let descriptors = UsbDescriptors {
        device: vec![],
        configurations: vec![],
    };
    match IphoneUsbHost::request_car_play_mode(&usb, &descriptors).unwrap() {
        CarPlayModeOutcome::Rejected { diagnostic, .. } => {
            assert!(diagnostic.contains("current mode=unreadable(0)"))
        }
        _ => panic!("Empty mode request must be rejected"),
    }
}
