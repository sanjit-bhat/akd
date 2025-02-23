use std::time::Instant;

use akd_core::ecvrf::{
    HardCodedAkdVRF as VRF, Proof, VRFExpandedPrivateKey, VRFKeyStorage, VRFPrivateKey, VRFPublicKey
};
use akd_core::verify::history::HistoryParams;
use akd_core::verify::{key_history_verify, lookup_verify, HistoryVerificationParams};
use akd_core::{AkdLabel, AkdValue, AzksElement, AzksValue, NodeLabel};
use rand::{rngs::StdRng, Rng, SeedableRng};

use akd::storage::memory::AsyncInMemoryDatabase as DB;
use akd::{
    append_only_zks::{AzksParallelismConfig, AzksParallelismOption, InsertMode},
    auditor::Auditor,
    storage::{manager::StorageManager, memory::AsyncInMemoryDatabase},
    Azks, Directory, EpochHash,
};
use sha2::{Digest, Sha256};

use akd_core::configuration::ExampleLabel;
use akd_core::ExperimentalConfiguration;

type TC = ExperimentalConfiguration<ExampleLabel>;

const NSEED: usize = 1_000_000;
const DEFAULT_DIG: [u8; 32] = [2; 32];
const PAR_CFG: AzksParallelismConfig = AzksParallelismConfig {
    insertion: AzksParallelismOption::Static(8),
    preload: AzksParallelismOption::Disabled,
};

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
        let elem = vec![(l.clone(), AkdValue(vec![2]))];
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
    // TODO: change par to use default, which gets all available cores.
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
        // TODO: change val to be 32 bytes. expected size.
        let new_els: Vec<(AkdLabel, AkdValue)> = (0..NINSERT)
            .map(|_| (AkdLabel::random(&mut rng), AkdValue(vec![2])))
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
            .map(|_| (AkdLabel::random(&mut rng), AkdValue(vec![2])))
            .collect();
        let _end_hash = dir.publish(new_els).await.unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_merk_prove() {
    let (mut rng, store, tr) = seed_tr().await;
    const NOPS: usize = 100_000;

    let start = Instant::now();
    for _ in 0..NOPS {
        let l = rand_label(&mut rng);
        tr.get_non_membership_proof::<TC, _>(&store, l)
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
async fn bench_merk_put() {
    let (mut rng, store, mut tr) = seed_tr().await;
    const NOPS: usize = 100_000;

    let start = Instant::now();
    for _ in 0..NOPS {
        let elems = vec![AzksElement {
            label: rand_label(&mut rng),
            value: AzksValue(DEFAULT_DIG),
        }];
        // no parallelism for single put.
        tr.batch_insert_nodes::<TC, _>(
            &store,
            elems,
            InsertMode::Directory,
            AzksParallelismConfig::disabled(),
        )
        .await
        .unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_micros() as f64) / (NOPS as f64);
    println!("us/op: {}", m0);
    println!("total ms: {}", total.as_millis());
}

async fn seed_tr() -> (StdRng, StorageManager<AsyncInMemoryDatabase>, Azks) {
    let mut rng = StdRng::seed_from_u64(42);
    let db = AsyncInMemoryDatabase::new();
    let store = StorageManager::new_no_cache(db);

    let mut tr = Azks::new::<TC, _>(&store).await.unwrap();
    let seed: Vec<AzksElement> = (0..NSEED)
        .map(|_| AzksElement {
            label: rand_label(&mut rng),
            value: AzksValue(DEFAULT_DIG),
        })
        .collect();
    tr.batch_insert_nodes::<TC, _>(&store, seed, InsertMode::Directory, PAR_CFG)
        .await
        .unwrap();
    (rng, store, tr)
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
        .map(|l| (l, AkdValue(vec![2])))
        .collect();
    dir.publish(seed).await.unwrap();
    (rng, dir, labels, h)
}

fn rand_label(rng: &mut StdRng) -> NodeLabel {
    NodeLabel {
        label_val: rng.gen::<[u8; 32]>(),
        label_len: 256,
    }
}
