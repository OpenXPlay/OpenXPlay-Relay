// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::{AirPlayCrypto, Result};
use num_bigint::BigUint;
use rand_core::{OsRng, RngCore};
pub struct Srp6a;
#[derive(Clone, Debug)]
pub struct Srp6aChallenge {
    pub salt: Vec<u8>,
    pub public_key: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Srp6aResult {
    pub ok: bool,
    pub session_key: Option<Vec<u8>>,
    pub server_proof: Option<Vec<u8>>,
}
pub struct Srp6aSession {
    salt: Vec<u8>,
    public_key_b: BigUint,
    identifier: Vec<u8>,
    verifier: BigUint,
    private_exponent: BigUint,
}
fn modulus() -> BigUint {
    BigUint::parse_bytes(N_HEX.as_bytes(), 16).unwrap()
}
fn pad(n: &BigUint) -> Vec<u8> {
    let mut b = n.to_bytes_be();
    if b.len() < 384 {
        let mut out = vec![0; 384 - b.len()];
        out.append(&mut b);
        out
    } else {
        b
    }
}
impl Srp6a {
    pub fn start(username: &str, password: &str) -> Result<Srp6aSession> {
        let mut salt = vec![0; 16];
        let mut private = [0; 32];
        OsRng
            .try_fill_bytes(&mut salt)
            .map_err(std::io::Error::other)?;
        OsRng
            .try_fill_bytes(&mut private)
            .map_err(std::io::Error::other)?;
        let n = modulus();
        let g = BigUint::from(5u8);
        let multiplier =
            BigUint::from_bytes_be(&AirPlayCrypto::sha512(&[&n.to_bytes_be(), &pad(&g)]));
        let x = BigUint::from_bytes_be(&AirPlayCrypto::sha512(&[
            &salt,
            &AirPlayCrypto::sha512(&[username.as_bytes(), b":", password.as_bytes()]),
        ]));
        let verifier = g.modpow(&x, &n);
        let private_exponent = BigUint::from_bytes_be(&private);
        private.fill(0);
        let public_key_b = (&multiplier * &verifier + g.modpow(&private_exponent, &n)) % &n;
        Ok(Srp6aSession {
            salt,
            public_key_b,
            identifier: username.as_bytes().to_vec(),
            verifier,
            private_exponent,
        })
    }
}
impl Srp6aSession {
    pub fn public_key(&self) -> Vec<u8> {
        pad(&self.public_key_b)
    }
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
    pub fn verify(&self, public_key_a: &[u8], client_proof: &[u8]) -> Result<Srp6aResult> {
        let fail = || Srp6aResult {
            ok: false,
            session_key: None,
            server_proof: None,
        };
        let a = BigUint::from_bytes_be(public_key_a);
        let n = modulus();
        if &a % &n == BigUint::from(0u8) {
            return Ok(fail());
        }
        let pa = pad(&a);
        let pb = pad(&self.public_key_b);
        let u = BigUint::from_bytes_be(&AirPlayCrypto::sha512(&[&pa, &pb]));
        let s = ((&a * self.verifier.modpow(&u, &n)) % &n).modpow(&self.private_exponent, &n);
        let key = AirPlayCrypto::sha512(&[&s.to_bytes_be()]);
        let hn = AirPlayCrypto::sha512(&[&n.to_bytes_be()]);
        let hg = AirPlayCrypto::sha512(&[&[5]]);
        let xor: Vec<u8> = hn.iter().zip(hg).map(|(a, b)| a ^ b).collect();
        let expected = AirPlayCrypto::sha512(&[
            &xor,
            &AirPlayCrypto::sha512(&[&self.identifier]),
            &self.salt,
            &pa,
            &pb,
            &key,
        ]);
        if expected != client_proof {
            return Ok(fail());
        }
        Ok(Srp6aResult {
            ok: true,
            session_key: Some(key.to_vec()),
            server_proof: Some(AirPlayCrypto::sha512(&[&pa, client_proof, &key]).to_vec()),
        })
    }
}
pub const SALT_BYTES: u64 = 16;
pub const PRIVATE_BYTES: u64 = 32;
pub const N_BYTES: u64 = 384;
/// RFC 5054 3072-bit group prime.
pub const N_HEX: &str = "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7EDEE386BFB5A899FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163BF0598DA48361C55D39A69163FA8FD24CF5F83655D23DCA3AD961C62F356208552BB9ED529077096966D670C354E4ABC9804F1746C08CA18217C32905E462E36CE3BE39E772C180E86039B2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF6955817183995497CEA956AE515D2261898FA051015728E5A8AAAC42DAD33170D04507A33A85521ABDF1CBA64ECFB850458DBEF0A8AEA71575D060C7DB3970F85A6E1E4C7ABF5AE8CDB0933D71E8C94E04A25619DCEE3D2261AD2EE6BF12FFA06D98A0864D87602733EC86A64521F2B18177B200CBBE117577A615D6C770988C0BAD946E208E24FA074E5AB3143DB5BFCE0FD108E4B82D120A93AD2CAFFFFFFFFFFFFFFFF";
