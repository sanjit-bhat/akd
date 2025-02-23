use std::time::Instant;

use akd_core::ecvrf::{
    HardCodedAkdVRF as VRF, Proof, VRFExpandedPrivateKey, VRFKeyStorage, VRFPrivateKey,
    VRFPublicKey,
};
use rand::{rngs::StdRng, Rng, SeedableRng};

use sha2::{Digest, Sha256};

#[test]
fn bench_rand32() {
    let mut data: [u8; 32] = [0; 32];
    let mut rng = StdRng::seed_from_u64(42);

    const NOPS: usize = 100_000_000;
    let start = Instant::now();
    for _ in 0..NOPS {
        rng.fill(&mut data);
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_nanos() as f64) / (NOPS as f64);
    println!("ns/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[test]
fn bench_rand64() {
    let mut data: [u8; 64] = [0; 64];
    let mut rng = StdRng::seed_from_u64(42);

    const NOPS: usize = 100_000_000;
    let start = Instant::now();
    for _ in 0..NOPS {
        rng.fill(&mut data);
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_nanos() as f64) / (NOPS as f64);
    println!("ns/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[test]
fn bench_hash() {
    let mut data: [u8; 64] = [2; 64];
    let mut rng = StdRng::seed_from_u64(42);

    const NOPS: usize = 10_000_000;
    let start = Instant::now();
    for _ in 0..NOPS {
        rng.fill(&mut data);
        Sha256::digest(data);
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_nanos() as f64) / (NOPS as f64);
    println!("ns/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_vrf_eval() {
    let (vrf_sk, vrf_pk) = init_vrf().await;
    let mut data: [u8; 32] = [0; 32];
    let mut rng = StdRng::seed_from_u64(42);
    const NOPS: usize = 50_000;

    let start = Instant::now();
    for _ in 0..NOPS {
        rng.fill(&mut data);
        vrf_sk.evaluate(&vrf_pk, &data);
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_vrf_prove() {
    let (vrf_sk, vrf_pk) = init_vrf().await;
    let mut data: [u8; 32] = [0; 32];
    let mut rng = StdRng::seed_from_u64(42);
    const NOPS: usize = 50_000;

    let start = Instant::now();
    for _ in 0..NOPS {
        rng.fill(&mut data);
        let p = vrf_sk.prove(&vrf_pk, &data);
        // byte enc for comparison with pav.
        let _pb = p.to_bytes();
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_vrf_verify() {
    let (vrf_sk, vrf_pk) = init_vrf().await;
    let mut data: [u8; 32] = [0; 32];
    let mut rng = StdRng::seed_from_u64(42);
    const NOPS: usize = 30_000;

    let start = Instant::now();
    for _ in 0..NOPS {
        rng.fill(&mut data);
        let p0 = vrf_sk.prove(&vrf_pk, &data);
        // byte enc for comparison with pav.
        let pb = p0.to_bytes();
        let p1 = Proof::try_from(&pb[..]).unwrap();
        vrf_pk.verify(&p1, &data).unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

async fn init_vrf() -> (VRFExpandedPrivateKey, VRFPublicKey) {
    let v0 = VRF {};
    let v1 = v0.retrieve().await.unwrap();
    let v2 = VRFPrivateKey::try_from(v1.as_slice()).unwrap();
    let vrf_sk = VRFExpandedPrivateKey::from(&v2);
    let vrf_pk = VRFPublicKey::from(&v2);
    (vrf_sk, vrf_pk)
}
