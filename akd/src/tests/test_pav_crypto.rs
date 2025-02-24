use std::time::Instant;

use akd_core::ecvrf::{
    HardCodedAkdVRF as VRF, Proof, VRFExpandedPrivateKey, VRFKeyStorage, VRFPrivateKey,
    VRFPublicKey,
};
use rand::{rngs::StdRng, Rng, SeedableRng};

use sha2::{Digest, Sha256};

use akd::benchutil::{report, Metric};

#[test]
fn bench_rand32() {
    let mut data: [u8; 32] = [0; 32];
    let mut rng = StdRng::seed_from_u64(42);

    let n_ops: i32 = 100_000_000;
    let start = Instant::now();
    for _ in 0..n_ops {
        rng.fill(&mut data);
    }
    let total = start.elapsed();

    let m0 = total.as_nanos() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_rand32".into(),
        n_ops,
        &[
            &Metric {
                n: m0,
                unit: "ns/op".into(),
            },
            &Metric {
                n: m1,
                unit: "total(ms)".into(),
            },
        ],
    );
}

#[test]
fn bench_rand64() {
    let mut data: [u8; 64] = [0; 64];
    let mut rng = StdRng::seed_from_u64(42);

    let n_ops = 100_000_000;
    let start = Instant::now();
    for _ in 0..n_ops {
        rng.fill(&mut data);
    }
    let total = start.elapsed();

    let m0 = total.as_nanos() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_rand64".into(),
        n_ops,
        &[
            &Metric {
                n: m0,
                unit: "ns/op".into(),
            },
            &Metric {
                n: m1,
                unit: "total(ms)".into(),
            },
        ],
    );
}

#[test]
fn bench_hash() {
    let mut data: [u8; 64] = [2; 64];
    let mut rng = StdRng::seed_from_u64(42);

    let n_ops = 10_000_000;
    let start = Instant::now();
    for _ in 0..n_ops {
        rng.fill(&mut data);
        Sha256::digest(data);
    }
    let total = start.elapsed();

    let m0 = total.as_nanos() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_hash".into(),
        n_ops,
        &[
            &Metric {
                n: m0,
                unit: "ns/op".into(),
            },
            &Metric {
                n: m1,
                unit: "total(ms)".into(),
            },
        ],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_vrf_eval() {
    let (vrf_sk, vrf_pk) = init_vrf().await;
    let mut data: [u8; 32] = [0; 32];
    let mut rng = StdRng::seed_from_u64(42);
    let n_ops = 50_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        rng.fill(&mut data);
        vrf_sk.evaluate(&vrf_pk, &data);
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_vrf_eval".into(),
        n_ops,
        &[
            &Metric {
                n: m0,
                unit: "ns/op".into(),
            },
            &Metric {
                n: m1,
                unit: "total(ms)".into(),
            },
        ],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_vrf_prove() {
    let (vrf_sk, vrf_pk) = init_vrf().await;
    let mut data: [u8; 32] = [0; 32];
    let mut rng = StdRng::seed_from_u64(42);
    let n_ops = 50_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        rng.fill(&mut data);
        let p = vrf_sk.prove(&vrf_pk, &data);
        // byte enc for comparison with pav.
        let _pb = p.to_bytes();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_vrf_prove".into(),
        n_ops,
        &[
            &Metric {
                n: m0,
                unit: "ns/op".into(),
            },
            &Metric {
                n: m1,
                unit: "total(ms)".into(),
            },
        ],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_vrf_verify() {
    let (vrf_sk, vrf_pk) = init_vrf().await;
    let mut data: [u8; 32] = [0; 32];
    let mut rng = StdRng::seed_from_u64(42);
    let n_ops = 30_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        rng.fill(&mut data);
        let p0 = vrf_sk.prove(&vrf_pk, &data);
        // byte enc for comparison with pav.
        let pb = p0.to_bytes();
        let p1 = Proof::try_from(&pb[..]).unwrap();
        vrf_pk.verify(&p1, &data).unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_vrf_verify".into(),
        n_ops,
        &[
            &Metric {
                n: m0,
                unit: "ns/op".into(),
            },
            &Metric {
                n: m1,
                unit: "total(ms)".into(),
            },
        ],
    );
}

async fn init_vrf() -> (VRFExpandedPrivateKey, VRFPublicKey) {
    let v0 = VRF {};
    let v1 = v0.retrieve().await.unwrap();
    let v2 = VRFPrivateKey::try_from(v1.as_slice()).unwrap();
    let vrf_sk = VRFExpandedPrivateKey::from(&v2);
    let vrf_pk = VRFPublicKey::from(&v2);
    (vrf_sk, vrf_pk)
}
