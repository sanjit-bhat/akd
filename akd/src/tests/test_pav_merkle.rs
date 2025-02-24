use std::time::Instant;

use akd_core::{AzksElement, AzksValue, NodeLabel};
use rand::{rngs::StdRng, Rng, SeedableRng};

use akd::benchutil::{report, Metric};
use akd::{
    append_only_zks::{AzksParallelismConfig, AzksParallelismOption, InsertMode},
    storage::{manager::StorageManager, memory::AsyncInMemoryDatabase},
    Azks,
};

use akd_core::configuration::ExampleLabel;
use akd_core::ExperimentalConfiguration;

type TC = ExperimentalConfiguration<ExampleLabel>;

const NSEED: usize = 1_000_000;
const DEFAULT_DIG: [u8; 32] = [2; 32];
const PAR_CFG: AzksParallelismConfig = AzksParallelismConfig {
    insertion: AzksParallelismOption::AvailableOr(0),
    preload: AzksParallelismOption::Disabled,
};

#[tokio::test(flavor = "multi_thread")]
async fn bench_merk_prove() {
    let (mut rng, store, tr) = seed_tr().await;
    let n_ops = 100_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        let l = rand_label(&mut rng);
        tr.get_non_membership_proof::<TC, _>(&store, l)
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_merk_prove".into(),
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
async fn bench_merk_put() {
    let (mut rng, store, mut tr) = seed_tr().await;
    let n_ops = 100_000;

    let start = Instant::now();
    for _ in 0..n_ops {
        let elems = vec![AzksElement {
            label: rand_label(&mut rng),
            value: AzksValue(DEFAULT_DIG),
        }];
        tr.batch_insert_nodes::<TC, _>(&store, elems, InsertMode::Directory, PAR_CFG)
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_merk_prove".into(),
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

fn rand_label(rng: &mut StdRng) -> NodeLabel {
    NodeLabel {
        label_val: rng.gen::<[u8; 32]>(),
        label_len: 256,
    }
}
