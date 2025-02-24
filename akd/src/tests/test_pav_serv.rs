use std::sync::Arc;
use std::time::Instant;

use akd_core::ecvrf::HardCodedAkdVRF as VRF;
use akd_core::verify::history::HistoryParams;
use akd_core::verify::lookup_verify;
use akd_core::{AkdLabel, AkdValue};
use rand::{rngs::StdRng, SeedableRng};
use rand::{CryptoRng, Rng};

use akd::benchutil::{report, Metric};
use akd::storage::memory::AsyncInMemoryDatabase as DB;
use akd::{
    append_only_zks::{AzksParallelismConfig, AzksParallelismOption},
    auditor::Auditor,
    storage::{manager::StorageManager, memory::AsyncInMemoryDatabase},
    Directory, EpochHash,
};

use akd_core::configuration::ExampleLabel;
use akd_core::ExperimentalConfiguration;

type TC = ExperimentalConfiguration<ExampleLabel>;

const DEF_NSEED: usize = 1_000_000;
const PAR_CFG: AzksParallelismConfig = AzksParallelismConfig {
    insertion: AzksParallelismOption::AvailableOr(0),
    preload: AzksParallelismOption::Disabled,
};

#[tokio::test(flavor = "multi_thread")]
async fn bench_serv_put_one() {
    let (mut rng, dir, _labels, _) = seed_server(DEF_NSEED).await;
    let n_ops = 2_000;
    let n_warm = get_warmup(n_ops);

    let mut start = Instant::now();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            start = Instant::now();
        }
        let l = mk_rand_label(&mut rng);
        let elem = vec![(l.clone(), mk_def_val())];
        dir.publish(elem).await.unwrap();
        dir.key_history(&l, HistoryParams::MostRecent(1))
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = (total.as_micros() as f64) / (n_ops as f64);
    let m1 = total.as_millis() as f64;
    report(
        "bench_serv_put_one".into(),
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
async fn bench_serv_put_batch() {
    let (mut rng, dir, _labels, _) = seed_server(DEF_NSEED).await;
    let n_ops = 100;
    let n_warm = get_warmup(n_ops);
    let n_insert = 1_000;

    let mut start = Instant::now();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            start = Instant::now();
        }

        let batch: Vec<(AkdLabel, AkdValue)> = (0..n_insert)
            .map(|_| (mk_rand_label(&mut rng), mk_def_val()))
            .collect();
        dir.publish(batch.clone()).await.unwrap();

        let mut join_set = tokio::task::JoinSet::new();
        for (label, _) in batch.into_iter() {
            let dir_clone = dir.clone();
            join_set.spawn(async move {
                dir_clone
                    .key_history(&label, HistoryParams::MostRecent(1))
                    .await
            });
        }
        while let Some(_) = join_set.join_next().await {}
    }
    let total = start.elapsed();

    let m0 = (total.as_micros() as f64) / (n_ops as f64);
    let m1 = total.as_millis() as f64;
    report(
        "bench_serv_put_batch".into(),
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
async fn bench_serv_get() {
    let (mut _rng, dir, labels, _) = seed_server(DEF_NSEED).await;
    let vrf_pk = dir.get_public_key().await.unwrap();
    let n_ops = 3_000;

    let start = Instant::now();
    for i in 0..n_ops {
        let l = &labels[i % DEF_NSEED];
        let (p, dig) = dir.lookup(l.clone()).await.unwrap();
        // note: each key only has one version.
        // note: run sequentially.
        lookup_verify::<TC>(vrf_pk.as_bytes(), dig.hash(), dig.epoch(), l.clone(), p).unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", n_ops);
    let m0 = (total.as_micros() as f64) / (n_ops as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_seed() {
    let s = Instant::now();
    seed_server(2_000_000).await;
    let t = s.elapsed();
    println!("{}", t.as_secs());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_audit() {
    let (mut rng, dir, _, fst_hash) = seed_server(DEF_NSEED).await;
    let mut aud = Auditor::<TC>::new(PAR_CFG).await;
    {
        let snd_hash = dir.get_epoch_hash().await.unwrap();
        let p = dir.audit(fst_hash.epoch(), snd_hash.epoch()).await.unwrap();
        aud.audit(vec![fst_hash.hash(), snd_hash.hash()], p)
            .await
            .unwrap();
    }
    let n_ops = 100;
    let n_insert = 1_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        let start_hash = dir.get_epoch_hash().await.unwrap();
        let new_els: Vec<(AkdLabel, AkdValue)> = (0..n_insert)
            .map(|_| (mk_rand_label(&mut rng), mk_def_val()))
            .collect();
        let end_hash = dir.publish(new_els).await.unwrap();

        let p = dir
            .audit(start_hash.epoch(), end_hash.epoch())
            .await
            .unwrap();
        aud.audit(vec![start_hash.hash(), end_hash.hash()], p)
            .await
            .unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", n_ops);
    let m0 = (total.as_micros() as f64) / (n_ops as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_audit_subtract() {
    let (mut rng, dir, _, _) = seed_server(DEF_NSEED).await;
    let n_ops = 100;
    let n_insert = 1_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        let _start_hash = dir.get_epoch_hash().await.unwrap();
        let new_els: Vec<(AkdLabel, AkdValue)> = (0..n_insert)
            .map(|_| (mk_rand_label(&mut rng), mk_def_val()))
            .collect();
        let _end_hash = dir.publish(new_els).await.unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", n_ops);
    let m0 = (total.as_micros() as f64) / (n_ops as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

async fn seed_server(
    nseed: usize,
) -> (
    StdRng,
    Arc<Directory<TC, DB, VRF>>,
    Vec<AkdLabel>,
    EpochHash,
) {
    let mut rng = StdRng::seed_from_u64(42);
    let db = AsyncInMemoryDatabase::new();
    let store = StorageManager::new_no_cache(db);
    let vrf = VRF {};
    let dir = Directory::<TC, _, _>::new(store, vrf, PAR_CFG)
        .await
        .unwrap();
    let h = dir.get_epoch_hash().await.unwrap();

    let labels: Vec<AkdLabel> = (0..nseed).map(|_| mk_rand_label(&mut rng)).collect();
    let seed: Vec<(AkdLabel, AkdValue)> = labels
        .clone()
        .into_iter()
        .map(|l| (l, mk_def_val()))
        .collect();
    dir.publish(seed).await.unwrap();
    (rng, Arc::new(dir), labels, h)
}

fn mk_rand_label<R: CryptoRng + Rng>(rng: &mut R) -> AkdLabel {
    // 8 bytes to match pav uint64 uid.
    let mut bytes = vec![0u8; 8];
    rng.fill_bytes(&mut bytes);
    AkdLabel(bytes)
}

fn mk_def_val() -> AkdValue {
    // 32 bytes for ed25519 pk.
    let mut v = vec![0; 32];
    for i in 0..32 {
        v[i] = 2
    }
    return AkdValue(v);
}

fn get_warmup(n_ops: i32) -> i32 {
    return (n_ops as f64 * 0.1) as i32;
}
