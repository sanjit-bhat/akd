use std::time::Instant;

use akd_core::ecvrf::HardCodedAkdVRF as VRF;
use akd_core::verify::history::HistoryParams;
use akd_core::verify::{key_history_verify, lookup_verify, HistoryVerificationParams};
use akd_core::{AkdLabel, AkdValue};
use rand::{rngs::StdRng, SeedableRng};

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

const NSEED: usize = 1_000_000;
const PAR_CFG: AzksParallelismConfig = AzksParallelismConfig {
    insertion: AzksParallelismOption::AvailableOr(0),
    preload: AzksParallelismOption::Disabled,
};

#[tokio::test(flavor = "multi_thread")]
async fn bench_serv_get() {
    let (mut _rng, dir, labels, _) = seed_dir(NSEED).await;
    let vrf_pk = dir.get_public_key().await.unwrap();
    const NOPS: usize = 3_000;

    let start = Instant::now();
    for i in 0..NOPS {
        let l = &labels[i % NSEED];
        let (p, dig) = dir.lookup(l.clone()).await.unwrap();
        // note: each key only has one version.
        // note: run sequentially.
        lookup_verify::<TC>(vrf_pk.as_bytes(), dig.hash(), dig.epoch(), l.clone(), p).unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_seed() {
    let s = Instant::now();
    seed_dir(2_000_000).await;
    let t = s.elapsed();
    println!("{}", t.as_secs());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_serv_put() {
    let (mut rng, dir, _labels, _) = seed_dir(NSEED).await;
    let vrf_pk = dir.get_public_key().await.unwrap();
    const NOPS: usize = 2_000;

    let start = Instant::now();
    for _ in 0..NOPS {
        let l = AkdLabel::random(&mut rng);
        let elem = vec![(l.clone(), mk_def_val())];
        dir.publish(elem).await.unwrap();
        let (p, dig) = dir
            .key_history(&l, HistoryParams::MostRecent(1))
            .await
            .unwrap();
        key_history_verify::<TC>(
            vrf_pk.as_bytes(),
            dig.hash(),
            dig.epoch(),
            l,
            p,
            HistoryVerificationParams::default(),
        )
        .unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_audit() {
    let (mut rng, dir, _, fst_hash) = seed_dir(NSEED).await;
    let mut aud = Auditor::<TC>::new(PAR_CFG).await;
    {
        let snd_hash = dir.get_epoch_hash().await.unwrap();
        let p = dir.audit(fst_hash.epoch(), snd_hash.epoch()).await.unwrap();
        aud.audit(vec![fst_hash.hash(), snd_hash.hash()], p)
            .await
            .unwrap();
    }
    const NOPS: usize = 100;
    const NINSERT: usize = 1_000;

    let start = Instant::now();
    for _ in 0..NOPS {
        let start_hash = dir.get_epoch_hash().await.unwrap();
        let new_els: Vec<(AkdLabel, AkdValue)> = (0..NINSERT)
            .map(|_| (AkdLabel::random(&mut rng), mk_def_val()))
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

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_audit_subtract() {
    let (mut rng, dir, _, _) = seed_dir(NSEED).await;
    const NOPS: usize = 100;
    const NINSERT: usize = 1_000;

    let start = Instant::now();
    for _ in 0..NOPS {
        let _start_hash = dir.get_epoch_hash().await.unwrap();
        let new_els: Vec<(AkdLabel, AkdValue)> = (0..NINSERT)
            .map(|_| (AkdLabel::random(&mut rng), mk_def_val()))
            .collect();
        let _end_hash = dir.publish(new_els).await.unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

async fn seed_dir(nseed: usize) -> (StdRng, Directory<TC, DB, VRF>, Vec<AkdLabel>, EpochHash) {
    let mut rng = StdRng::seed_from_u64(42);
    let db = AsyncInMemoryDatabase::new();
    let store = StorageManager::new_no_cache(db);
    let vrf = VRF {};
    let dir = Directory::<TC, _, _>::new(store, vrf, PAR_CFG)
        .await
        .unwrap();
    let h = dir.get_epoch_hash().await.unwrap();

    let labels: Vec<AkdLabel> = (0..nseed).map(|_| AkdLabel::random(&mut rng)).collect();
    let seed: Vec<(AkdLabel, AkdValue)> = labels
        .clone()
        .into_iter()
        .map(|l| (l, mk_def_val()))
        .collect();
    dir.publish(seed).await.unwrap();
    (rng, dir, labels, h)
}

fn mk_def_val() -> AkdValue {
    let mut v = vec![0; 32];
    for i in 0..32 {
        v[i] = 2
    }
    return AkdValue(v);
}
