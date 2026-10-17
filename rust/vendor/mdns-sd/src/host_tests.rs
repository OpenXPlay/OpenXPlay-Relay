// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Created by riko chen on 2026-10-16

use super::*;
use if_addrs::{Ifv4Addr, Ifv6Addr};

const HOST: &str = "openxplay-linux.local.";

fn interface(name: &str, address: &str, prefixlen: u8) -> Interface {
    Interface {
        name: name.into(),
        addr: IfAddr::V4(Ifv4Addr {
            ip: address.parse().unwrap(),
            netmask: Ipv4Addr::from(u32::MAX << (32 - prefixlen)),
            prefixlen,
            broadcast: None,
        }),
        index: Some(if name == "wlan0" { 1 } else { 2 }),
        #[cfg(windows)]
        adapter_name: name.into(),
    }
}

fn service(addresses: &str, intf: &Interface) -> ServiceInfo {
    let mut info = ServiceInfo::new(
        "_openxplay._tcp.local.",
        "OpenXPlay",
        HOST,
        addresses,
        6975,
        None::<HashMap<String, String>>,
    )
    .unwrap()
    .enable_addr_interface_only();
    info.set_status(intf, ServiceStatus::Announced);
    info
}

fn query(name: &str, types: &[RRType], intf: &Interface) -> DnsIncoming {
    let mut out = DnsOutgoing::new(FLAGS_QR_QUERY);
    for ty in types {
        out.add_question(name, *ty);
    }
    decode(&out, intf)
}

fn decode(out: &DnsOutgoing, intf: &Interface) -> DnsIncoming {
    let packets = out.to_data_on_wire();
    assert_eq!(packets.len(), 1);
    DnsIncoming::new(packets[0].clone(), intf.into()).unwrap()
}

fn respond(
    msg: &DnsIncoming,
    services: &[ServiceInfo],
    intf: &Interface,
    registry: &DnsRegistry,
) -> DnsOutgoing {
    let mut out = DnsOutgoing::new(FLAGS_QR_RESPONSE | FLAGS_AA);
    for question in msg.questions() {
        add_answer_of_host(
            &mut out,
            msg,
            question.entry_name(),
            question.entry_type(),
            services.iter(),
            intf,
            registry,
        );
    }
    out
}

#[test]
fn ipv4_host_answers_aaaa_with_wire_nsec() {
    let intf = interface("wlan0", "192.168.1.2", 24);
    let info = service("192.168.1.2", &intf);
    let out = respond(
        &query(HOST, &[RRType::AAAA], &intf),
        &[info],
        &intf,
        &DnsRegistry::new(),
    );
    let wire = &out.to_data_on_wire()[0];
    assert_eq!(&wire[wire.len() - 5..], &[0xc0, 0x0c, 0, 1, 0x40]);
    let reply = decode(&out, &intf);
    assert_eq!(reply.answers().len(), 1);
    let record = &reply.answers()[0];
    assert_eq!(record.get_name(), HOST);
    assert_eq!(record.get_type(), RRType::NSEC);
    assert_eq!(record.get_record().get_ttl(), 120);
    assert!(record.get_record().is_unique());
    assert_eq!(
        record.any().downcast_ref::<DnsNSec>().unwrap()._types(),
        vec![1]
    );
    assert!(reply.questions().is_empty());
}

#[test]
fn ipv4_a_and_any_queries_still_answer_a() {
    let intf = interface("wlan0", "192.168.1.2", 24);
    for ty in [RRType::A, RRType::ANY] {
        let out = respond(
            &query(HOST, &[ty], &intf),
            &[service("192.168.1.2", &intf)],
            &intf,
            &DnsRegistry::new(),
        );
        let reply = decode(&out, &intf);
        assert_eq!(reply.answers().len(), 1);
        assert_eq!(reply.answers()[0].get_type(), RRType::A);
    }
}

#[test]
fn combined_a_aaaa_query_gets_both_answers() {
    let intf = interface("wlan0", "192.168.1.2", 24);
    let out = respond(
        &query(HOST, &[RRType::A, RRType::AAAA], &intf),
        &[service("192.168.1.2", &intf)],
        &intf,
        &DnsRegistry::new(),
    );
    let reply = decode(&out, &intf);
    let types: Vec<_> = reply.answers().iter().map(|r| r.get_type()).collect();
    assert_eq!(types, vec![RRType::A, RRType::NSEC]);
}

#[test]
fn unknown_and_unclaimed_names_get_no_negative_answer() {
    let intf = interface("wlan0", "192.168.1.2", 24);
    let mut info = service("192.168.1.2", &intf);
    let registry = DnsRegistry::new();
    let out = respond(
        &query("other.local.", &[RRType::AAAA], &intf),
        &[info.clone()],
        &intf,
        &registry,
    );
    assert_eq!(out.answers_count(), 0);
    info.set_status(&intf, ServiceStatus::Probing);
    let out = respond(
        &query(HOST, &[RRType::AAAA], &intf),
        &[info],
        &intf,
        &registry,
    );
    assert_eq!(out.answers_count(), 0);
}

#[test]
fn renamed_host_is_matched_case_insensitively() {
    let intf = interface("wlan0", "192.168.1.2", 24);
    let info = service("192.168.1.2", &intf);
    let mut registry = DnsRegistry::new();
    registry
        .name_changes
        .insert(HOST.into(), "openxplay-linux-2.local.".into());
    let out = respond(
        &query(HOST, &[RRType::AAAA], &intf),
        &[info.clone()],
        &intf,
        &registry,
    );
    assert_eq!(out.answers_count(), 0);
    let out = respond(
        &query("OPENXPLAY-LINUX-2.LOCAL.", &[RRType::AAAA], &intf),
        &[info],
        &intf,
        &registry,
    );
    assert_eq!(
        decode(&out, &intf).answers()[0].get_name(),
        "openxplay-linux-2.local."
    );
}

#[test]
fn another_services_ipv6_address_prevents_false_negative() {
    let intf = interface("wlan0", "192.168.1.2", 24);
    let mut v6 = service("fe80::1234", &intf);
    v6.set_status(&intf, ServiceStatus::Unknown);
    let services = [service("192.168.1.2", &intf), v6];
    let out = respond(
        &query(HOST, &[RRType::AAAA], &intf),
        &services,
        &intf,
        &DnsRegistry::new(),
    );
    assert_eq!(out.answers_count(), 0);
}

#[test]
fn known_nsec_answer_is_suppressed() {
    let intf = interface("wlan0", "192.168.1.2", 24);
    let info = service("192.168.1.2", &intf);
    let mut known = DnsOutgoing::new(FLAGS_QR_QUERY);
    known.add_question(HOST, RRType::AAAA);
    known.add_answer_at_time(
        DnsNSec::new(HOST, CLASS_IN, 120, HOST.into(), vec![0x40]),
        0,
    );
    let out = respond(&decode(&known, &intf), &[info], &intf, &DnsRegistry::new());
    assert_eq!(out.answers_count(), 0);
    assert_eq!(out.known_answer_count(), 1);
}

#[test]
fn interface_addresses_never_cross_in_replies_or_announcements() {
    // Include overlapping subnets: subnet membership alone is insufficient.
    for wlan_ip in ["192.168.1.2", "192.168.68.2"] {
        let wlan = interface("wlan0", wlan_ip, 24);
        let usb = interface("usb0", "192.168.68.1", 24);
        let mut info = service(&format!("{wlan_ip},192.168.68.1"), &wlan);
        info.set_status(&usb, ServiceStatus::Announced);
        info.set_requires_probe(false);
        for intf in [&wlan, &usb] {
            let mut registry = DnsRegistry::new();
            let reply = respond(
                &query(HOST, &[RRType::A], intf),
                &[info.clone()],
                intf,
                &registry,
            );
            let announce = prepare_announce(&info, intf, &mut registry).unwrap();
            for out in [reply, announce] {
                let decoded = decode(&out, intf);
                let addresses: Vec<_> = decoded
                    .answers()
                    .iter()
                    .chain(decoded.additionals())
                    .filter_map(|r| r.any().downcast_ref::<DnsAddress>())
                    .map(DnsAddress::address)
                    .collect();
                assert_eq!(addresses, vec![intf.ip()]);
            }
        }
    }
}

#[test]
fn no_local_address_gets_no_negative_answer() {
    let intf = interface("wlan0", "192.168.1.2", 24);
    let out = respond(
        &query(HOST, &[RRType::AAAA], &intf),
        &[service("192.168.68.1", &intf)],
        &intf,
        &DnsRegistry::new(),
    );
    assert_eq!(out.answers_count(), 0);
}

#[test]
fn ipv6_host_answers_aaaa_and_denies_a_with_correct_bitmap() {
    let mut intf = interface("wlan0", "192.168.1.2", 24);
    intf.addr = IfAddr::V6(Ifv6Addr {
        ip: "fe80::1234".parse().unwrap(),
        netmask: Ipv6Addr::from(u128::MAX << 64),
        prefixlen: 64,
        broadcast: None,
    });
    let info = service("fe80::1234", &intf);
    for ty in [RRType::AAAA, RRType::A] {
        let out = respond(
            &query(HOST, &[ty], &intf),
            &[info.clone()],
            &intf,
            &DnsRegistry::new(),
        );
        let reply = decode(&out, &intf);
        assert_eq!(reply.answers().len(), 1);
        let record = &reply.answers()[0];
        if ty == RRType::AAAA {
            assert_eq!(record.get_type(), RRType::AAAA);
        } else {
            assert_eq!(record.get_type(), RRType::NSEC);
            assert_eq!(
                record.any().downcast_ref::<DnsNSec>().unwrap()._types(),
                vec![28]
            );
        }
    }
}
