use rand::prelude::IteratorRandom;
use std::future::Future;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::task::JoinSet;

use akd_core::ecvrf::HardCodedAkdVRF as VRF;
use akd_core::verify::history::HistoryParams;
use akd_core::verify::{key_history_verify, lookup_verify, HistoryVerificationParams};
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

    let m0 = total.as_micros() as f64 / n_ops as f64;
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
    for sz in [1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000] {
        put_batch_helper(sz).await;
    }
}

async fn put_batch_helper(batch_size: i32) {
    let (mut rng, dir, _labels, _) = seed_server(DEF_NSEED).await;
    let n_batches = 20;
    let n_warm = get_warmup(n_batches);

    let mut start = Instant::now();
    for i in 0..n_warm + n_batches {
        if i == n_warm {
            start = Instant::now();
        }

        let batch: Vec<(AkdLabel, AkdValue)> = (0..batch_size)
            .map(|_| (mk_rand_label(&mut rng), mk_def_val()))
            .collect();
        dir.publish(batch.clone()).await.unwrap();

        let mut join_set = JoinSet::new();
        for (label, _) in batch.into_iter() {
            let dir_clone = dir.clone();
            join_set.spawn(async move {
                dir_clone
                    .key_history(&label, HistoryParams::MostRecent(1))
                    .await
            });
        }
        while let Some(res) = join_set.join_next().await {
            let _ = res.unwrap();
        }
    }
    let total = start.elapsed();

    let tput = (n_batches * batch_size) as f64 / total.as_secs_f64();
    let lat = total.as_micros() as f64 / n_batches as f64;
    let overall = total.as_millis() as f64;
    report(
        "bench_serv_put_batch".into(),
        batch_size,
        &[
            &Metric {
                n: tput,
                unit: "op/s".into(),
            },
            &Metric {
                n: lat,
                unit: "us/batch".into(),
            },
            &Metric {
                n: overall,
                unit: "total(ms)".into(),
            },
        ],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_serv_put_verify() {
    let (mut _rng, dir, labels, _) = seed_server(DEF_NSEED).await;
    let vrf_pk = dir.get_public_key().await.unwrap();
    let n_ops = 2_000;
    let n_warm = get_warmup(n_ops);

    let mut total: Duration = Default::default();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            total = Default::default();
        }
        let l = &labels[i as usize % DEF_NSEED];
        let (p, dig) = dir
            .key_history(l, HistoryParams::MostRecent(1))
            .await
            .unwrap();

        let s = Instant::now();
        key_history_verify::<TC>(
            vrf_pk.as_bytes(),
            dig.hash(),
            dig.epoch(),
            l.clone(),
            p,
            HistoryVerificationParams::default(),
        )
        .unwrap();
        total += s.elapsed();
    }

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_serv_put_verify".into(),
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
async fn bench_serv_put_size() {
    let (mut rng, dir, _labels, _) = seed_server(DEF_NSEED).await;
    let l = mk_rand_label(&mut rng);
    let elem = vec![(l.clone(), mk_def_val())];
    dir.publish(elem).await.unwrap();
    let (p, dig) = dir
        .key_history(&l, HistoryParams::MostRecent(1))
        .await
        .unwrap();
    let pb = bincode::serialize(&p).unwrap();
    let digb = bincode::serialize(&dig).unwrap();
    let sz = (pb.len() + digb.len()) as f64;
    report(
        "bench_serv_put_size".into(),
        1,
        &[&Metric {
            n: sz,
            unit: "B".into(),
        }],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_serv_get_one() {
    let (mut _rng, dir, labels, _) = seed_server(DEF_NSEED).await;
    let n_ops = 3_000;
    let n_warm = get_warmup(n_ops);

    let mut start = Instant::now();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            start = Instant::now();
        }
        let l = &labels[i as usize % DEF_NSEED];
        dir.lookup(l.clone()).await.unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_serv_get_one".into(),
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
async fn bench_serv_get_size_one() {
    let (mut _rng, dir, labels, _) = seed_server(DEF_NSEED).await;
    let l = &labels[0];
    dir.lookup(l.clone()).await.unwrap();
    let (p, dig) = dir
        .key_history(l, HistoryParams::MostRecent(1))
        .await
        .unwrap();
    let pb = bincode::serialize(&p).unwrap();
    let digb = bincode::serialize(&dig).unwrap();
    let sz = (pb.len() + digb.len()) as f64;
    report(
        "bench_serv_put_size".into(),
        1,
        &[&Metric {
            n: sz,
            unit: "B".into(),
        }],
    );
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
        // TODO: time to not count this.
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

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_serv_audit".into(),
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

#[tokio::test(flavor = "multi_thread")]
async fn bench_serv_get_scale() {
    let (mut _rng, dir, labels, _) = seed_server(DEF_NSEED).await;
    let arc_labels = Arc::new(labels);
    let max_n_cli: usize = thread::available_parallelism().unwrap().into();
    let mut runner = ClientRunner::new(max_n_cli);

    for n_cli in 1..=max_n_cli {
        let dir_clone0 = dir.clone();
        let labels_clone0 = arc_labels.clone();

        let _total_time = runner.run(n_cli, move || {
            let dir_clone1 = dir_clone0.clone();
            let labels_clone1 = labels_clone0.clone();

            async move {
                let l = labels_clone1
                    .iter()
                    .choose(&mut rand::thread_rng())
                    .unwrap();
                dir_clone1.lookup(l.clone()).await.unwrap();
            }
        });
    }
}

#[derive(Debug, Clone, Copy)]
struct StartEnd {
    start: Instant,
    end: Instant,
}

struct Sample {
    xs: Vec<f64>,
}

struct ClientRunner {
    times: Vec<Arc<Mutex<Vec<StartEnd>>>>,
    sample: Sample,
}

impl ClientRunner {
    fn new(max_n_cli: usize) -> Self {
        let mut times = Vec::with_capacity(max_n_cli);
        for _ in 0..max_n_cli {
            times.push(Arc::new(Mutex::new(Vec::with_capacity(1_000_000))));
        }
        let sample = Sample {
            xs: Vec::with_capacity(10_000_000),
        };
        ClientRunner { times, sample }
    }

    async fn run<F, Fut>(&mut self, n_cli: usize, work: F) -> Duration
    where
        F: Fn() -> Fut + Send + 'static + Clone,
        Fut: Future + Send,
    {
        // Clear previous results
        for i in 0..n_cli {
            self.times[i].lock().await.clear();
        }

        // Get data.
        let mut join_set = JoinSet::new();
        for i in 0..n_cli {
            let times_clone = Arc::clone(&self.times[i]);
            let work_clone = work.clone();

            join_set.spawn(async move {
                let begin = Instant::now();
                let mut times = times_clone.lock().await;

                loop {
                    let start = Instant::now();
                    work_clone().await;
                    let end = Instant::now();
                    times.push(StartEnd { start, end });

                    if end - begin >= Duration::from_secs(1) {
                        break;
                    }
                }
            });
        }
        while let Some(res) = join_set.join_next().await {
            let _ = res.unwrap();
        }

        // Calculate time bounds
        let mut starts = Vec::with_capacity(n_cli);
        let mut ends = Vec::with_capacity(n_cli);
        for i in 0..n_cli {
            let times = self.times[i].lock().await;
            if let Some(first) = times.first() {
                starts.push(first.start);
            }
            if let Some(last) = times.last() {
                ends.push(last.end);
            }
        }

        let init = starts.into_iter().max().unwrap();
        let post_warm = init + Duration::from_millis(100);
        let end = ends.into_iter().min().unwrap();
        let total = end - post_warm;

        // Collect samples
        self.sample.xs.clear();
        for i in 0..n_cli {
            let times = self.times[i].lock().await;
            let low = times.partition_point(|s| s.start < post_warm);
            let high = times.partition_point(|s| s.end <= end);
            if (high as i64) - (low as i64) < 1_000 {
                panic!("ClientRunner: clients don't have enough overlapping samples");
            }

            for j in low..high {
                let d = (times[j].end - times[j].start).as_nanos() as f64;
                self.sample.xs.push(d);
            }
        }

        self.sample.xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        total
    }
}
