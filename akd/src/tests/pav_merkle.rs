use rand::prelude::IteratorRandom;
use rand::RngCore;
use std::time::Instant;

use akd_core::{AzksElement, AzksValue, NodeLabel};

use akd::storage::memory::AsyncInMemoryDatabase as DB;

use akd::benchutil::{report, Metric};
use akd::{
    append_only_zks::{AzksParallelismConfig, AzksParallelismOption, InsertMode},
    storage::{manager::StorageManager, memory::AsyncInMemoryDatabase},
    Azks,
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
async fn bench_merk_insert() {
    let (store, mut tr, _) = seed_tr().await;
    let n_ops = 100_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        let elem = vec![AzksElement {
            label: mk_rand_label(),
            value: mk_rand_val(),
        }];
        tr.batch_insert_nodes::<TC, _>(&store, elem, InsertMode::Directory, PAR_CFG)
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_merk_insert".into(),
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
async fn bench_merk_memb() {
    let (store, tr, labels) = seed_tr().await;
    let n_ops = 100_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        let l = labels.iter().choose(&mut rand::thread_rng()).unwrap();
        tr.get_membership_proof::<TC, DB>(&store, l.clone())
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_merk_memb".into(),
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
async fn bench_merk_nonmemb() {
    let (store, tr, labels) = seed_tr().await;
    let n_ops = 100_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        let l = labels.iter().choose(&mut rand::thread_rng()).unwrap();
        tr.get_non_membership_proof::<TC, DB>(&store, l.clone())
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_merk_nonmemb".into(),
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

async fn seed_tr() -> (StorageManager<DB>, Azks, Vec<NodeLabel>) {
    let db = AsyncInMemoryDatabase::new();
    let store = StorageManager::new_no_cache(db);
    let mut tr = Azks::new::<TC, _>(&store).await.unwrap();

    let labels: Vec<NodeLabel> = (0..DEF_NSEED).map(|_| mk_rand_label()).collect();
    let seed: Vec<AzksElement> = labels
        .clone()
        .into_iter()
        .map(|l| AzksElement {
            label: l,
            value: mk_rand_val(),
        })
        .collect();

    tr.batch_insert_nodes::<TC, _>(&store, seed, InsertMode::Directory, PAR_CFG)
        .await
        .unwrap();
    (store, tr, labels)
}

fn mk_rand_label() -> NodeLabel {
    let mut b = [0; 32];
    rand::thread_rng().fill_bytes(&mut b);
    NodeLabel {
        label_val: b,
        label_len: 256,
    }
}

fn mk_rand_val() -> AzksValue {
    let mut b = [0; 32];
    rand::thread_rng().fill_bytes(&mut b);
    AzksValue(b)
}
