use rand::RngCore;
use std::time::{Duration, Instant};

use akd_core::{
    ecvrf::{HardCodedAkdVRF as VRF, VRFKeyStorage},
    verify::base::verify_label,
    AkdLabel, ExampleLabel, ExperimentalConfiguration, VersionFreshness,
};
use rand::{rngs::StdRng, Rng, SeedableRng};

use sha2::{Digest, Sha256};

use akd::benchutil::{report, Metric};
type TC = ExperimentalConfiguration<ExampleLabel>;

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
async fn bench_label_get() {
    let mut uid = AkdLabel(vec![0; 8]);
    let vrf = VRF {};
    let n_ops = 50_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        rand::thread_rng().fill_bytes(&mut uid);
        vrf.get_node_label::<TC>(&uid, VersionFreshness::Fresh, 1)
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_label_get".into(),
        n_ops,
        &[
            &Metric {
                n: m0,
                unit: "us/op".into(),
            },
            &Metric {
                n: m1,
                unit: "total(ms)".into(),
            },
        ],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_label_gen_ver() {
    let mut uid = AkdLabel(vec![0; 8]);
    let vrf_sk = VRF {};
    let vrf_pk = vrf_sk.get_vrf_public_key().await.unwrap();
    let vrf_pkb = vrf_pk.to_bytes();
    let n_ops = 50_000;

    let mut total_gen = Duration::default();
    let mut total_ver = Duration::default();
    for _ in 0..n_ops {
        rand::thread_rng().fill_bytes(&mut uid);

        let t0 = Instant::now();
        let p = vrf_sk
            .get_label_proof::<TC>(&uid, VersionFreshness::Fresh, 1)
            .await
            .unwrap();
        // AKD calls this right after. include in time.
        let pb = p.to_bytes();
        total_gen += t0.elapsed();

        let label = vrf_sk.get_node_label_from_vrf_proof(p).await;

        let t1 = Instant::now();
        verify_label::<TC>(&vrf_pkb, &uid, VersionFreshness::Fresh, 1, &pb, label).unwrap();
        total_ver += t1.elapsed();
    }

    let m0 = total_gen.as_micros() as f64 / n_ops as f64;
    let m1 = total_gen.as_millis() as f64;
    let m2 = total_ver.as_micros() as f64 / n_ops as f64;
    let m3 = total_ver.as_millis() as f64;
    report(
        "bench_label_gen_ver".into(),
        n_ops,
        &[
            &Metric {
                n: m0,
                unit: "us/op(gen)".into(),
            },
            &Metric {
                n: m1,
                unit: "total(ms,gen)".into(),
            },
            &Metric {
                n: m2,
                unit: "us/op(ver)".into(),
            },
            &Metric {
                n: m3,
                unit: "total(ms,ver)".into(),
            },
        ],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_label_size() {
    let uid = AkdLabel(vec![0; 8]);
    let vrf_sk = VRF {};
    let p = vrf_sk
        .get_label_proof::<TC>(&uid, VersionFreshness::Fresh, 1)
        .await
        .unwrap();
    let pb = p.to_bytes();
    report(
        "bench_label_size".into(),
        1,
        &[&Metric {
            n: pb.len() as f64,
            unit: "B".into(),
        }],
    );
}
