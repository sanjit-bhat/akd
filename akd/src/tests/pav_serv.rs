use akd::storage::memory::AsyncInMemoryDatabase as DB;
use rand::RngCore;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;

use akd::storage::memory::AsyncInMemoryDatabase;
use akd::storage::StorageManager;
use akd::Directory;

use akd_core::ecvrf::HardCodedAkdVRF as VRF;
use rand::prelude::IteratorRandom;
use std::thread;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

use akd_core::verify::history::HistoryParams;
use akd_core::verify::{key_history_verify, lookup_verify, HistoryVerificationParams};
use akd_core::{AkdLabel, AkdValue};

use akd::benchutil::{report, Metric};
use akd::{
    append_only_zks::{AzksParallelismConfig, AzksParallelismOption},
    auditor::Auditor,
};

use akd_core::configuration::ExampleLabel;
use akd_core::ExperimentalConfiguration;

type TC = ExperimentalConfiguration<ExampleLabel>;

const DEF_NSEED: usize = 1_000_000;
const PAR_CFG: AzksParallelismConfig = AzksParallelismConfig {
    insertion: AzksParallelismOption::AvailableOr(0),
    preload: AzksParallelismOption::Disabled,
};
const NS_PER_US: f64 = 1_000.0;

#[tokio::test(flavor = "multi_thread")]
async fn bench_put_one() {
    let (serv, _labels, _) = seed_server(DEF_NSEED).await;
    let n_ops = 10_000;
    let n_warm = get_warmup(n_ops);

    let mut start = Instant::now();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            start = Instant::now();
        }
        let l = mk_rand_label();
        let elem = vec![(l.clone(), mk_rand_val())];
        serv.publish(elem).await.unwrap();
        serv.key_history(&l, HistoryParams::MostRecent(1))
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_put_one".into(),
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
async fn bench_put_batch() {
    for batch_sz in [1, 2, 5, 10, 20, 50, 100, 200, 500, 1_000, 2_000, 5_000] {
        let (serv, _labels, _) = seed_server(DEF_NSEED).await;
        let (total, n_batches) = put_batch_helper(&serv, batch_sz).await;

        let tput = (n_batches * batch_sz) as f64 / total.as_secs_f64();
        let lat = total.as_micros() as f64 / n_batches as f64;
        let overall = total.as_millis() as f64;
        report(
            "bench_put_batch".into(),
            batch_sz,
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
}

async fn put_batch_helper(serv: &Arc<Directory<TC, DB, VRF>>, batch_sz: i32) -> (Duration, i32) {
    let n_batches = 20;
    let n_warm = get_warmup(n_batches);

    let mut start = Instant::now();
    for i in 0..n_warm + n_batches {
        if i == n_warm {
            start = Instant::now();
        }

        let batch: Vec<(AkdLabel, AkdValue)> = (0..batch_sz)
            .map(|_| (mk_rand_label(), mk_rand_val()))
            .collect();
        serv.publish(batch.clone()).await.unwrap();

        let mut join_set = JoinSet::new();
        for (label, _) in batch.into_iter() {
            let serv_clone = serv.clone();
            join_set.spawn(async move {
                serv_clone
                    .key_history(&label, HistoryParams::MostRecent(1))
                    .await
            });
        }
        while let Some(res) = join_set.join_next().await {
            let _ = res.unwrap();
        }
    }
    let total = start.elapsed();
    (total, n_batches)
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_put_verify() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let vrf_pk = serv.get_public_key().await.unwrap();
    let n_ops = 10_000;
    let n_warm = get_warmup(n_ops);

    let mut total: Duration = Default::default();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            total = Default::default();
        }
        let l = labels.iter().choose(&mut rand::thread_rng()).unwrap();
        let (p, dig) = serv
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
            HistoryVerificationParams::Default {
                history_params: HistoryParams::MostRecent(1),
            },
            None,
        )
        .unwrap();
        total += s.elapsed();
    }

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_put_verify".into(),
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
async fn bench_put_size() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let l = &labels[0];
    let (p, dig) = serv
        .key_history(&l, HistoryParams::MostRecent(1))
        .await
        .unwrap();
    let pb = bincode::serialize(&p).unwrap();
    let digb = bincode::serialize(&dig).unwrap();
    let sz = (pb.len() + digb.len()) as f64;
    report(
        "bench_put_size".into(),
        1,
        &[&Metric {
            n: sz,
            unit: "B".into(),
        }],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_get_one() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let n_ops = 10_000;
    let n_warm = get_warmup(n_ops);

    let mut start = Instant::now();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            start = Instant::now();
        }
        let l = labels.iter().choose(&mut rand::thread_rng()).unwrap();
        serv.lookup(l.clone()).await.unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_get_one".into(),
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
async fn bench_get_scale() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let max_n_cli: usize = thread::available_parallelism().unwrap().into();
    let mut run = ClientRunner::new(max_n_cli);

    for n_cli in 1..=max_n_cli {
        let serv_clone0 = serv.clone();
        let labels_clone0 = labels.clone();

        let total_time = run
            .run(n_cli, move || {
                let serv_clone1 = serv_clone0.clone();
                let labels_clone1 = labels_clone0.clone();

                async move {
                    let l = labels_clone1
                        .iter()
                        .choose(&mut rand::thread_rng())
                        .unwrap();
                    serv_clone1.lookup(l.clone()).await.unwrap();
                }
            })
            .await;

        let ops = run.sample.weight();
        let tput = ops as f64 / total_time.as_secs_f64();
        report(
            "bench_get_scale".into(),
            n_cli as i32,
            &[
                &Metric {
                    n: tput,
                    unit: "op/s".into(),
                },
                &Metric {
                    n: run.sample.mean() / NS_PER_US,
                    unit: "mean(us)".into(),
                },
                &Metric {
                    n: run.sample.stddev() / NS_PER_US,
                    unit: "stddev".into(),
                },
                &Metric {
                    n: run.sample.quantile(0.99) / NS_PER_US,
                    unit: "p99".into(),
                },
            ],
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_get_size() {
    let (serv, _labels, _) = seed_server(DEF_NSEED).await;
    let max_n_vers = 10;
    let label = mk_rand_label();

    for n_vers in 1..=max_n_vers {
        let elem = vec![(label.clone(), mk_rand_val())];
        serv.publish(elem.clone()).await.unwrap();

        let (p, dig) = serv.lookup(label.clone()).await.unwrap();
        if p.version != n_vers {
            panic!("wrong version");
        }
        let pb = bincode::serialize(&p).unwrap();
        let digb = bincode::serialize(&dig).unwrap();
        let sz = (pb.len() + digb.len()) as f64;
        report(
            "bench_get_size".into(),
            n_vers as i32,
            &[&Metric {
                n: sz,
                unit: "B".into(),
            }],
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_get_verify() {
    let max_n_vers = 10;
    for n_vers in 1..=max_n_vers {
        let (n_ops, total_gen, total_verify) = get_verify_helper(n_vers).await;

        let m0 = total_gen.as_micros() as f64 / n_ops as f64;
        let m1 = total_gen.as_millis() as f64;
        let m2 = total_verify.as_micros() as f64 / n_ops as f64;
        let m3 = total_verify.as_millis() as f64;

        report(
            "bench_get_verify".into(),
            n_vers,
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
}

async fn get_verify_helper(n_vers: i32) -> (i32, Duration, Duration) {
    let (serv, _, _) = seed_server(DEF_NSEED).await;
    let vrf_pk = serv.get_public_key().await.unwrap().to_bytes();
    let n_ops = 10_000;
    let n_warm = get_warmup(n_ops);
    let mut total_gen: Duration = Default::default();
    let mut total_verify: Duration = Default::default();

    for i in 0..n_warm + n_ops {
        if i == n_warm {
            total_gen = Default::default();
            total_verify = Default::default();
        }

        let l = mk_rand_label();
        for _ in 0..n_vers {
            let elem = vec![(l.clone(), mk_rand_val())];
            serv.publish(elem.clone()).await.unwrap();
        }

        let s0 = Instant::now();
        let (p, dig) = serv.lookup(l.clone()).await.unwrap();
        total_gen += s0.elapsed();

        if p.version as i32 != n_vers {
            panic!("wrong version");
        }

        let s1 = Instant::now();
        lookup_verify::<TC>(&vrf_pk, dig.hash(), dig.epoch(), l.clone(), p).unwrap();
        total_verify += s1.elapsed();
    }
    (n_ops, total_gen, total_verify)
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_selfmon_one() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let n_ops = 20_000;
    let n_warm = get_warmup(n_ops);

    let mut start = Instant::now();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            start = Instant::now();
        }
        let l = labels.iter().choose(&mut rand::thread_rng()).unwrap();
        serv.key_history(&l, HistoryParams::MostRecent(0))
            .await
            .unwrap();
    }
    let total = start.elapsed();

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_selfmon_one".into(),
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
async fn bench_selfmon_scale() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let max_n_cli: usize = thread::available_parallelism().unwrap().into();
    let mut run = ClientRunner::new(max_n_cli);

    for n_cli in 1..=max_n_cli {
        let serv_clone0 = serv.clone();
        let labels_clone0 = labels.clone();

        let total_time = run
            .run(n_cli, move || {
                let serv_clone1 = serv_clone0.clone();
                let labels_clone1 = labels_clone0.clone();

                async move {
                    let l = labels_clone1
                        .iter()
                        .choose(&mut rand::thread_rng())
                        .unwrap();
                    serv_clone1
                        .key_history(&l, HistoryParams::MostRecent(0))
                        .await
                        .unwrap();
                }
            })
            .await;

        let ops = run.sample.weight();
        let tput = ops as f64 / total_time.as_secs_f64();
        report(
            "bench_selfmon_scale".into(),
            n_cli as i32,
            &[
                &Metric {
                    n: tput,
                    unit: "op/s".into(),
                },
                &Metric {
                    n: run.sample.mean() / NS_PER_US,
                    unit: "mean(us)".into(),
                },
                &Metric {
                    n: run.sample.stddev() / NS_PER_US,
                    unit: "stddev".into(),
                },
                &Metric {
                    n: run.sample.quantile(0.99) / NS_PER_US,
                    unit: "p99".into(),
                },
            ],
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_selfmon_size() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let l = &labels[0];
    let (p, dig) = serv
        .key_history(&l, HistoryParams::MostRecent(0))
        .await
        .unwrap();
    let pb = bincode::serialize(&p).unwrap();
    let digb = bincode::serialize(&dig).unwrap();
    let sz = (pb.len() + digb.len()) as f64;
    report(
        "bench_selfmon_size".into(),
        1,
        &[&Metric {
            n: sz,
            unit: "B".into(),
        }],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_selfmon_verify() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let vrf_pk = serv.get_public_key().await.unwrap().to_bytes();
    let n_ops = 20_000;
    let n_warm = get_warmup(n_ops);

    let mut total: Duration = Default::default();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            total = Default::default();
        }
        let l = labels.iter().choose(&mut rand::thread_rng()).unwrap();
        let (p, dig) = serv
            .key_history(&l, HistoryParams::MostRecent(0))
            .await
            .unwrap();

        let s = Instant::now();
        key_history_verify::<TC>(
            &vrf_pk,
            dig.hash(),
            dig.epoch(),
            l.clone(),
            p,
            HistoryVerificationParams::Default {
                history_params: HistoryParams::MostRecent(0),
            },
            Some(1),
        )
        .unwrap();
        total += s.elapsed();
    }

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_selfmon_verify".into(),
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
async fn bench_audit_batch() {
    let (serv, _, mut aud) = seed_server(DEF_NSEED).await;
    let n_ops = 300;
    let n_warm = get_warmup(n_ops);
    let n_insert = 1_000;

    let mut total: Duration = Default::default();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            total = Default::default();
        }
        let start_dig = serv.get_epoch_hash().await.unwrap();
        let new_els: Vec<(AkdLabel, AkdValue)> = (0..n_insert)
            .map(|_| (mk_rand_label(), mk_rand_val()))
            .collect();
        let end_dig = serv.publish(new_els).await.unwrap();

        let s = Instant::now();
        let p = serv
            .audit(start_dig.epoch(), end_dig.epoch())
            .await
            .unwrap();
        aud.audit(vec![start_dig.hash(), end_dig.hash()], p)
            .await
            .unwrap();
        total += s.elapsed();
    }

    let m0 = total.as_micros() as f64 / n_ops as f64;
    let m1 = total.as_millis() as f64;
    report(
        "bench_audit_batch".into(),
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
async fn bench_audit_size() {
    let (serv, _, _) = seed_server(DEF_NSEED).await;
    let n_insert = 1_000;

    let start_dig = serv.get_epoch_hash().await.unwrap();
    let new_els: Vec<(AkdLabel, AkdValue)> = (0..n_insert)
        .map(|_| (mk_rand_label(), mk_rand_val()))
        .collect();
    let end_dig = serv.publish(new_els).await.unwrap();
    let p = serv
        .audit(start_dig.epoch(), end_dig.epoch())
        .await
        .unwrap();

    let pb = bincode::serialize(&p).unwrap();
    let sz = pb.len() as f64;
    report(
        "bench_audit_size".into(),
        1,
        &[&Metric {
            n: sz,
            unit: "B".into(),
        }],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_serv_scale() {
    let db = AsyncInMemoryDatabase::new();
    let store = StorageManager::new_no_cache(db);
    let vrf = VRF {};
    let serv = Directory::<TC, _, _>::new(store, vrf, PAR_CFG)
        .await
        .unwrap();
    let n_insert = 500_000_000;
    let n_measure = 500_000;
    let n_ops = 10_000;
    let n_warm = get_warmup(n_ops);

    let mut sys_info = sysinfo::System::new();
    let pid = sysinfo::get_current_pid().unwrap();
    sys_info.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    sys_info.refresh_memory();

    for i in (0..n_insert).step_by(n_measure) {
        sys_info.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        sys_info.refresh_memory();

        let mut start = Instant::now();
        for j in 0..n_warm + n_ops {
            if j == n_warm {
                start = Instant::now();
            }
            let l = mk_rand_label();
            let elem = vec![(l.clone(), mk_rand_val())];
            serv.publish(elem).await.unwrap();
            serv.key_history(&l, HistoryParams::MostRecent(1))
                .await
                .unwrap();
        }
        let total = start.elapsed();

        let n_rem = n_measure as i32 - n_warm - n_ops;
        let rem: Vec<(AkdLabel, AkdValue)> = (0..n_rem)
            .map(|_| (mk_rand_label(), mk_rand_val()))
            .collect();
        serv.publish(rem).await.unwrap();

        let lat = total.as_micros() as f64 / n_ops as f64;
        let mb0 = sys_info.process(pid).unwrap().memory() as f64 / 1_000_000.0;
        let mb1 = sys_info.used_memory() as f64 / 1_000_000.0;
        report(
            "bench_serv_scale".into(),
            i as i32,
            &[
                &Metric {
                    n: lat,
                    unit: "us/op".into(),
                },
                &Metric {
                    n: mb0,
                    unit: "MB(proc)".into(),
                },
                &Metric {
                    n: mb1,
                    unit: "MB(sys)".into(),
                },
            ],
        );
    }
}

pub async fn seed_server(
    n_seed: usize,
) -> (Arc<Directory<TC, DB, VRF>>, Arc<Vec<AkdLabel>>, Auditor<TC>) {
    let db = AsyncInMemoryDatabase::new();
    let store = StorageManager::new_no_cache(db);
    let vrf = VRF {};
    let serv = Directory::<TC, _, _>::new(store, vrf, PAR_CFG)
        .await
        .unwrap();
    let mut aud = Auditor::<TC>::new(PAR_CFG).await;
    let labels: Vec<AkdLabel> = (0..n_seed).map(|_| mk_rand_label()).collect();

    // WhatsApp actually has around 1M epochs (as of 2025-02-28),
    // but 65_536 is the biggest future marker version less than that.
    // see get_marker_versions.
    let n_ep = 65_536;
    if n_seed < n_ep {
        panic!("n_seed too small");
    }
    for i in 0..n_ep {
        let elem = vec![(labels[i].clone(), mk_rand_val())];
        let start_dig = serv.get_epoch_hash().await.unwrap();
        serv.publish(elem).await.unwrap();
        let end_dig = serv.get_epoch_hash().await.unwrap();
        let p = serv
            .audit(start_dig.epoch(), end_dig.epoch())
            .await
            .unwrap();
        aud.audit(vec![start_dig.hash(), end_dig.hash()], p)
            .await
            .unwrap();
    }

    let rem: Vec<(AkdLabel, AkdValue)> = labels[n_ep..]
        .iter()
        .map(|l| (l.clone(), mk_rand_val()))
        .collect();
    {
        let start_dig = serv.get_epoch_hash().await.unwrap();
        serv.publish(rem).await.unwrap();
        let end_dig = serv.get_epoch_hash().await.unwrap();
        let p = serv
            .audit(start_dig.epoch(), end_dig.epoch())
            .await
            .unwrap();
        aud.audit(vec![start_dig.hash(), end_dig.hash()], p)
            .await
            .unwrap();
    }
    (Arc::new(serv), Arc::new(labels), aud)
}

pub fn mk_rand_label() -> AkdLabel {
    // 8 bytes to match pav uint64 uid.
    let mut bytes = vec![0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    AkdLabel(bytes)
}

pub fn mk_rand_val() -> AkdValue {
    // 32 bytes for ed25519 pk.
    let mut v = vec![0; 32];
    rand::thread_rng().fill_bytes(&mut v);
    AkdValue(v)
}

pub fn get_warmup(n_ops: i32) -> i32 {
    return (n_ops as f64 * 0.1) as i32;
}

#[derive(Debug, Clone, Copy)]
struct StartEnd {
    start: Instant,
    end: Instant,
}

// Rust port of
// https://github.com/aclements/go-moremath/blob/f10218a/stats/sample.go,
// without weighting.
pub struct Sample {
    xs: Vec<f64>,
}

impl Sample {
    pub fn mean(&self) -> f64 {
        if self.xs.len() == 0 {
            return f64::NAN;
        }
        let mut m: f64 = 0.0;
        for (i, x) in self.xs.iter().enumerate() {
            m += (x - m) / (i + 1) as f64;
        }
        m
    }

    fn variance(&self) -> f64 {
        if self.xs.len() == 0 {
            return f64::NAN;
        } else if self.xs.len() <= 1 {
            return 0.0;
        }

        let mut mean = 0.0;
        let mut m2 = 0.0;
        for (n, x) in self.xs.iter().enumerate() {
            let delta = x - mean;
            mean += delta / (n + 1) as f64;
            m2 += delta * (x - mean);
        }
        return m2 / (self.xs.len() - 1) as f64;
    }

    pub fn stddev(&self) -> f64 {
        return self.variance().sqrt();
    }

    fn golang_modf(f: f64) -> (f64, f64) {
        return (f.trunc(), f.fract());
    }

    pub fn quantile(&self, q: f64) -> f64 {
        if self.xs.len() == 0 {
            return f64::NAN;
        } else if q <= 0.0 {
            return *self.xs.first().unwrap();
        } else if q >= 1.0 {
            return *self.xs.last().unwrap();
        }

        let big_n = self.xs.len() as f64;
        let n = 1.0 / 3.0 + q * (big_n + 1.0 / 3.0);
        let (kf, frac) = Self::golang_modf(n);
        let k = kf as i64;
        if k <= 0 {
            return *self.xs.first().unwrap();
        } else if k as usize >= self.xs.len() {
            return *self.xs.last().unwrap();
        }
        return self.xs[(k - 1) as usize]
            + frac * (self.xs[k as usize] - self.xs[(k - 1) as usize]);
    }

    pub fn weight(&self) -> usize {
        return self.xs.len();
    }
}

pub struct ClientRunner {
    times: Vec<Arc<Mutex<Vec<StartEnd>>>>,
    pub sample: Sample,
}

impl ClientRunner {
    pub fn new(max_n_cli: usize) -> Self {
        let mut times = Vec::with_capacity(max_n_cli);
        for _ in 0..max_n_cli {
            times.push(Arc::new(Mutex::new(Vec::with_capacity(1_000_000))));
        }
        let sample = Sample {
            xs: Vec::with_capacity(10_000_000),
        };
        ClientRunner { times, sample }
    }

    pub async fn run<F, Fut>(&mut self, n_cli: usize, work: F) -> Duration
    where
        F: Fn() -> Fut + Send + 'static + Clone,
        Fut: Future + Send,
    {
        for i in 0..n_cli {
            self.times[i].lock().await.clear();
        }

        // Get data.
        let mut join_set = JoinSet::new();
        for i in 0..n_cli {
            let times_clone = Arc::clone(&self.times[i]);
            let work_clone = work.clone();

            join_set.spawn(async move {
                let mut times = times_clone.lock().await;
                let cli_begin = Instant::now();

                loop {
                    let start = Instant::now();
                    work_clone().await;
                    let end = Instant::now();
                    times.push(StartEnd { start, end });

                    if end - cli_begin >= Duration::from_secs(1) {
                        break;
                    }
                }
            });
        }
        while let Some(res) = join_set.join_next().await {
            let _ = res.unwrap();
        }

        // Calculate time bounds.
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

        // Collect samples.
        self.sample.xs.clear();
        for i in 0..n_cli {
            let times = self.times[i].lock().await;
            let low = times.partition_point(|s| s.start < post_warm);
            let high = times.partition_point(|s| s.end <= end);
            if (high as i64) - (low as i64) < 200 {
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
