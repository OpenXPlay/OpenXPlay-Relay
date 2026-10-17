// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
use carplay_airplay::*;
use carplay_iap2::mfi::{BaaCertificatePair, MfiCertificateType};
use num_bigint::BigUint;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
fn hash(parts: &[&[u8]]) -> Vec<u8> {
    AirPlayCrypto::sha512(parts).to_vec()
}
fn pad(n: &BigUint) -> Vec<u8> {
    let bytes = n.to_bytes_be();
    let mut out = vec![0; 384usize.saturating_sub(bytes.len())];
    out.extend(bytes);
    out
}
fn derive(key: &[u8], salt: &str, info: &str) -> [u8; 32] {
    AirPlayCrypto::hkdf_sha512(key, salt.as_bytes(), info.as_bytes(), 32)
        .unwrap()
        .try_into()
        .unwrap()
}
fn tlv(items: &[(u8, Vec<u8>)]) -> Vec<u8> {
    Tlv8Codec::encode(
        &items
            .iter()
            .map(|(kind, value)| Tlv8Item {
                kind: *kind,
                value: value.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
#[derive(Default)]
struct Store(Mutex<BTreeMap<String, [u8; 32]>>);
impl PairingStore for Store {
    fn save(&self, id: &str, key: &[u8; 32]) -> Result<()> {
        self.0.lock().unwrap().insert(id.into(), *key);
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Option<[u8; 32]>> {
        Ok(self.0.lock().unwrap().get(id).copied())
    }
    fn clear(&self) -> Result<()> {
        self.0.lock().unwrap().clear();
        Ok(())
    }
}
fn identity() -> Arc<AirPlayIdentity> {
    let k = AirPlayCrypto::ed25519_generate().unwrap();
    Arc::new(AirPlayIdentity {
        private_key: k.private_key,
        public_key: k.public_key,
        pairing_id: "accessory-id".into(),
    })
}
struct ClientProof {
    a: Vec<u8>,
    proof: Vec<u8>,
    key: Vec<u8>,
}
fn client_proof(salt: &[u8], b: &[u8], password: &str) -> ClientProof {
    let n = BigUint::parse_bytes(carplay_airplay::srp6a::N_HEX.as_bytes(), 16).unwrap();
    let g = BigUint::from(5u8);
    let private = BigUint::from_bytes_be(&[0x43; 32]);
    let a = g.modpow(&private, &n);
    let b = BigUint::from_bytes_be(b);
    let multiplier = BigUint::from_bytes_be(&hash(&[&n.to_bytes_be(), &pad(&g)]));
    let x = BigUint::from_bytes_be(&hash(&[
        salt,
        &hash(&[b"Pair-Setup:", password.as_bytes()]),
    ]));
    let u = BigUint::from_bytes_be(&hash(&[&pad(&a), &pad(&b)]));
    let base = (&b + &n - ((&multiplier * g.modpow(&x, &n)) % &n)) % &n;
    let secret = base.modpow(&(&private + u * &x), &n);
    let key = hash(&[&secret.to_bytes_be()]);
    let xor: Vec<_> = hash(&[&n.to_bytes_be()])
        .iter()
        .zip(hash(&[&[5]]))
        .map(|(a, b)| a ^ b)
        .collect();
    let proof = hash(&[
        &xor,
        &hash(&[b"Pair-Setup"]),
        salt,
        &pad(&a),
        &pad(&b),
        &key,
    ]);
    ClientProof {
        a: pad(&a),
        proof,
        key,
    }
}
#[test]
fn srp_client_and_server_agree_and_authenticate_both_proofs() {
    let server = Srp6a::start("Pair-Setup", "3939").unwrap();
    assert_eq!(server.salt().len(), 16);
    assert_eq!(server.public_key().len(), 384);
    let client = client_proof(server.salt(), &server.public_key(), "3939");
    let result = server.verify(&client.a, &client.proof).unwrap();
    assert!(result.ok);
    assert_eq!(result.session_key.as_ref().unwrap(), &client.key);
    assert_eq!(
        result.server_proof.unwrap(),
        hash(&[&client.a, &client.proof, &client.key])
    );
}
#[test]
fn srp_rejects_zero_modulus_and_wrong_password() {
    let server = Srp6a::start("Pair-Setup", "3939").unwrap();
    assert!(!server.verify(&[0], &[0; 64]).unwrap().ok);
    let n = BigUint::parse_bytes(carplay_airplay::srp6a::N_HEX.as_bytes(), 16).unwrap();
    assert!(!server.verify(&n.to_bytes_be(), &[0; 64]).unwrap().ok);
    let client = client_proof(server.salt(), &server.public_key(), "bad");
    assert!(!server.verify(&client.a, &client.proof).unwrap().ok);
}
#[test]
fn pair_setup_full_exchange_persists_only_signed_controller() {
    let identity = identity();
    let store = Arc::new(Store::default());
    let mut setup = PairSetup::new(identity.clone(), store.clone());
    let m2 = Tlv8Codec::decode(&setup.handle(&tlv(&[(6, vec![1])])).unwrap()).unwrap();
    assert_eq!(m2[&6], [2]);
    let client = client_proof(&m2[&2], &m2[&3], "3939");
    let m4 = Tlv8Codec::decode(
        &setup
            .handle(&tlv(&[
                (6, vec![3]),
                (3, client.a.clone()),
                (4, client.proof.clone()),
            ]))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(m4[&6], [4]);
    assert_eq!(m4[&4], hash(&[&client.a, &client.proof, &client.key]));
    assert!(store.0.lock().unwrap().is_empty());
    let controller = AirPlayCrypto::ed25519_generate().unwrap();
    let id = b"controller-id";
    let signing_key = derive(
        &client.key,
        "Pair-Setup-Controller-Sign-Salt",
        "Pair-Setup-Controller-Sign-Info",
    );
    let signature = AirPlayCrypto::ed25519_sign(
        &controller.private_key,
        &[signing_key.as_slice(), id, &controller.public_key].concat(),
    )
    .unwrap();
    let encryption_key = derive(
        &client.key,
        "Pair-Setup-Encrypt-Salt",
        "Pair-Setup-Encrypt-Info",
    );
    let sub = tlv(&[
        (1, id.to_vec()),
        (3, controller.public_key.to_vec()),
        (10, signature.to_vec()),
    ]);
    let sealed = AirPlayCrypto::chacha_seal(
        &encryption_key,
        &AirPlayCrypto::nonce_label("PS-Msg05"),
        &sub,
        &[],
    )
    .unwrap();
    let m6 = Tlv8Codec::decode(&setup.handle(&tlv(&[(6, vec![5]), (5, sealed)])).unwrap()).unwrap();
    assert_eq!(m6[&6], [6]);
    assert!(setup.complete);
    assert_eq!(
        store.get("controller-id").unwrap(),
        Some(controller.public_key)
    );
    let sub = Tlv8Codec::decode(
        &AirPlayCrypto::chacha_open(
            &encryption_key,
            &AirPlayCrypto::nonce_label("PS-Msg06"),
            &m6[&5],
            &[],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(sub[&1], b"accessory-id");
    assert_eq!(sub[&3], identity.public_key);
    let signing_key = derive(
        &client.key,
        "Pair-Setup-Accessory-Sign-Salt",
        "Pair-Setup-Accessory-Sign-Info",
    );
    assert!(AirPlayCrypto::ed25519_verify(
        &identity.public_key,
        &[signing_key.as_slice(), &sub[&1], &sub[&3]].concat(),
        &sub[&10]
    ));
}
#[test]
fn pair_setup_missing_fields_fail_with_source_states() {
    let mut setup = PairSetup::new(identity(), Arc::new(Store::default()));
    assert_eq!(
        setup.handle(&tlv(&[(6, vec![3])])).unwrap(),
        tlv(&[(6, vec![4]), (7, vec![2])])
    );
    assert_eq!(
        setup.handle(&tlv(&[(6, vec![5])])).unwrap(),
        tlv(&[(6, vec![6]), (7, vec![2])])
    );
    assert_eq!(
        setup.handle(&[]).unwrap(),
        tlv(&[(6, vec![0]), (7, vec![2])])
    );
    assert!(!setup.complete);
}
fn begin_verify(
    identity: Arc<AirPlayIdentity>,
    store: Arc<Store>,
) -> (PairVerify, X25519KeyPair, [u8; 32], [u8; 32], Vec<u8>) {
    let mut verify = PairVerify::new(identity.clone(), store);
    let client = AirPlayCrypto::x25519_generate().unwrap();
    let m2 = Tlv8Codec::decode(
        &verify
            .handle(&tlv(&[(6, vec![1]), (3, client.public_key.to_vec())]))
            .unwrap(),
    )
    .unwrap();
    let own: [u8; 32] = m2[&3].clone().try_into().unwrap();
    let shared = AirPlayCrypto::x25519_shared(&client.private_key, &own).unwrap();
    let key = derive(
        &shared,
        "Pair-Verify-Encrypt-Salt",
        "Pair-Verify-Encrypt-Info",
    );
    let sub = Tlv8Codec::decode(
        &AirPlayCrypto::chacha_open(&key, &AirPlayCrypto::nonce_label("PV-Msg02"), &m2[&5], &[])
            .unwrap(),
    )
    .unwrap();
    assert!(AirPlayCrypto::ed25519_verify(
        &identity.public_key,
        &[own.as_slice(), &sub[&1], &client.public_key].concat(),
        &sub[&10]
    ));
    assert!(verify.control_cipher().is_err());
    (verify, client, shared, key, own.to_vec())
}
#[test]
fn pair_verify_checks_long_term_signature_and_key_directions() {
    let store = Arc::new(Store::default());
    let controller = AirPlayCrypto::ed25519_generate().unwrap();
    store.save("controller", &controller.public_key).unwrap();
    let (mut verify, client, shared, key, own) = begin_verify(identity(), store);
    let signature = AirPlayCrypto::ed25519_sign(
        &controller.private_key,
        &[client.public_key.as_slice(), b"controller", &own].concat(),
    )
    .unwrap();
    let sub = tlv(&[(1, b"controller".to_vec()), (10, signature.to_vec())]);
    let sealed =
        AirPlayCrypto::chacha_seal(&key, &AirPlayCrypto::nonce_label("PV-Msg03"), &sub, &[])
            .unwrap();
    assert_eq!(
        verify.handle(&tlv(&[(6, vec![3]), (5, sealed)])).unwrap(),
        tlv(&[(6, vec![4])])
    );
    assert_eq!(verify.shared(), Some(shared));
    assert_eq!(
        verify.verified_controller_id().as_deref(),
        Some("controller")
    );
    let mut server = verify.control_cipher().unwrap();
    let mut client = ControlCipher::new(
        derive(&shared, "Control-Salt", "Control-Read-Encryption-Key"),
        derive(&shared, "Control-Salt", "Control-Write-Encryption-Key"),
    );
    assert_eq!(
        server
            .decrypt(&client.encrypt(b"request").unwrap())
            .unwrap()
            .data,
        b"request"
    );
    assert_eq!(
        client
            .decrypt(&server.encrypt(b"response").unwrap())
            .unwrap()
            .data,
        b"response"
    );
}
#[test]
fn pair_verify_unknown_or_bad_signatures_never_enable_control_cipher() {
    let store = Arc::new(Store::default());
    let (mut verify, _, _, key, _) = begin_verify(identity(), store);
    let sub = tlv(&[(1, b"unknown".to_vec()), (10, vec![0; 64])]);
    let sealed =
        AirPlayCrypto::chacha_seal(&key, &AirPlayCrypto::nonce_label("PV-Msg03"), &sub, &[])
            .unwrap();
    assert_eq!(
        verify.handle(&tlv(&[(6, vec![3]), (5, sealed)])).unwrap(),
        tlv(&[(6, vec![4]), (7, vec![2])])
    );
    assert!(verify.control_cipher().is_err());
    assert!(verify.verified_controller_id().is_none());
}
#[test]
fn x25519_rejects_noncontributory_peer() {
    assert!(AirPlayCrypto::x25519_shared(&[1; 32], &[0; 32]).is_err());
}
#[test]
fn ed25519_rejects_bad_signature_and_key_sizes() {
    let key = AirPlayCrypto::ed25519_generate().unwrap();
    let sig = AirPlayCrypto::ed25519_sign(&key.private_key, b"message").unwrap();
    assert!(AirPlayCrypto::ed25519_verify(
        &key.public_key,
        b"message",
        &sig
    ));
    assert!(!AirPlayCrypto::ed25519_verify(
        &key.public_key,
        b"tampered",
        &sig
    ));
    assert!(!AirPlayCrypto::ed25519_verify(
        &key.public_key,
        b"message",
        &sig[..63]
    ));
}
struct Auth {
    major: u8,
    challenge: Mutex<Vec<u8>>,
    intermediate: usize,
}
impl MfiAuthenticator for Auth {
    fn certificate_type(&self) -> MfiCertificateType {
        if self.intermediate > 0 {
            MfiCertificateType::Baa
        } else {
            MfiCertificateType::Mfi
        }
    }

    fn protocol_major(&self) -> Result<u8> {
        Ok(self.major)
    }
    fn read_certificate(&self, _: usize) -> Result<Vec<u8>> {
        Ok(vec![1, 2, 3])
    }
    fn sign_challenge(&self, data: &[u8]) -> Result<Vec<u8>> {
        *self.challenge.lock().unwrap() = data.to_vec();
        Ok(vec![0x42; 64])
    }
    fn baa_certificates(&self) -> Result<BaaCertificatePair> {
        Ok(BaaCertificatePair {
            leaf: vec![7, 8],
            intermediate: vec![0x99; self.intermediate],
        })
    }
}
#[test]
fn sap_mfi_uses_protocol_digest_and_exact_aes_ctr_labels() {
    use ctr::cipher::{KeyIvInit, StreamCipher};
    use sha2::Digest;
    for major in [2, 3] {
        let auth = Auth {
            major,
            challenge: Mutex::new(vec![]),
            intermediate: 0,
        };
        let client = AirPlayCrypto::x25519_generate().unwrap();
        let body = [&[1][..], &client.public_key].concat();
        let response = MfiSapAuthSetup::handle(&body, &auth).unwrap().unwrap();
        let own: [u8; 32] = response[..32].try_into().unwrap();
        let transcript = [own, client.public_key].concat();
        let expected = if major == 2 {
            sha1::Sha1::digest(&transcript).to_vec()
        } else {
            sha2::Sha256::digest(&transcript).to_vec()
        };
        assert_eq!(*auth.challenge.lock().unwrap(), expected);
        assert_eq!(&response[32..39], &[0, 0, 0, 3, 1, 2, 3]);
        assert_eq!(&response[39..43], &64u32.to_be_bytes());
        let shared = AirPlayCrypto::x25519_shared(&client.private_key, &own).unwrap();
        let key = sha1::Sha1::digest([b"AES-KEY".as_slice(), &shared].concat());
        let iv = sha1::Sha1::digest([b"AES-IV".as_slice(), &shared].concat());
        let mut signature = response[43..].to_vec();
        ctr::Ctr128BE::<aes::Aes128>::new_from_slices(&key[..16], &iv[..16])
            .unwrap()
            .apply_keystream(&mut signature);
        assert_eq!(signature, vec![0x42; 64]);
    }
}
#[test]
fn sap_baa_packages_leaf_raw_transcript_and_opack_intermediate() {
    for len in [0, 32, 33, 255, 256, 65535, 65536] {
        let auth = Auth {
            major: 3,
            challenge: Mutex::new(vec![]),
            intermediate: len,
        };
        let client = AirPlayCrypto::x25519_generate().unwrap();
        let response = MfiSapAuthSetup::handle_with_certificate_type(
            &[&[1][..], &client.public_key].concat(),
            &auth,
            MfiCertificateType::Baa,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            *auth.challenge.lock().unwrap(),
            [&response[..32], &client.public_key].concat()
        );
        if len > 0 {
            let automatic =
                MfiSapAuthSetup::handle(&[&[1][..], &client.public_key].concat(), &auth)
                    .unwrap()
                    .unwrap();
            assert_eq!(automatic.len(), response.len());
        }

        assert_eq!(&response[32..38], &[0, 0, 0, 2, 7, 8]);
        let offset = 38 + 4 + 64;
        let blob_len =
            u32::from_be_bytes(response[offset..offset + 4].try_into().unwrap()) as usize;
        let blob = &response[offset + 4..];
        assert_eq!(blob.len(), blob_len);
        assert_eq!(&blob[..6], &[0xe1, 0x44, b'b', b'a', b'I', b'C']);
        assert!(blob.ends_with(&vec![0x99; len]));
        assert_eq!(
            blob[6],
            match len {
                0..=32 => 0x70 + len as u8,
                33..=255 => 0x91,
                256..=65535 => 0x92,
                _ => 0x93,
            }
        );
    }
}
#[test]
fn sap_malformed_requests_never_call_authenticator() {
    let auth = Auth {
        major: 3,
        challenge: Mutex::new(vec![]),
        intermediate: 0,
    };
    for body in [
        vec![],
        vec![1; 32],
        vec![2; 33],
        vec![1; 34],
        [vec![1], vec![0; 32]].concat(),
    ] {
        assert!(MfiSapAuthSetup::handle(&body, &auth).unwrap().is_none());
    }
    assert!(auth.challenge.lock().unwrap().is_empty());
}
