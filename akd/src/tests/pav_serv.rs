use akd::directory::get_marker_version;
use akd::storage::memory::AsyncInMemoryDatabase as DB;
use akd_core::utils::get_marker_versions;
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

use akd::benchutil::*;
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

#[test]
fn test_print_markers() {
    for ver in 1..=50 {
        let (past, future) = get_marker_versions(ver, ver, 500_000);
        println!("{}: {:?}-{:?}", ver, past, future);
    }
}

/*
some output from the below test:
put: 1, [2, 4, 16, 256, 65536]; get: 8, 8
put: 2, [3, 4, 16, 256, 65536]; get: 8, 8
put: 3, [4, 16, 256, 65536]; get: 8, 8
put: 4, [5, 6, 8, 16, 256, 65536]; get: 7, 4
put: 5, [6, 8, 16, 256, 65536]; get: 7, 4
put: 6, [7, 8, 16, 256, 65536]; get: 32, 32
put: 7, [8, 16, 256, 65536]; get: 32, 32
put: 8, [9, 10, 12, 16, 256, 65536]; get: 11, 8
put: 9, [10, 12, 16, 256, 65536]; get: 11, 8
put: 10, [11, 12, 16, 256, 65536]; get: 13, 8
*/
#[test]
fn test_marker_attack() {
    let max_scan = 50;
    let n_epochs = 500_000;
    for put_ver in 1..=max_scan {
        // called when a user Alice SelfMon's her own key, at put_ver.
        // for actual call, see directory::key_history.
        let (_, put_fut_markers) = get_marker_versions(put_ver, put_ver, n_epochs);
        for get_ver in put_ver + 1..put_ver + 1 + max_scan {
            if put_fut_markers.contains(&get_ver) {
                continue;
            }
            // called when a different user Get's Alice's key,
            // and the server tries to lie and say it's at get_ver.
            // for actual call, see directory::build_lookup_info,
            // called from directory::lookup.
            let get_past_marker = 1 << get_marker_version(get_ver);
            if put_fut_markers.contains(&get_past_marker) {
                continue;
            }
            println!(
                "put: {}, {:?}; get: {}, {}",
                put_ver, put_fut_markers, get_ver, get_past_marker
            );
            // NOTE: lot more get_ver's that satisfy,
            // but skip those for presentation.
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_marker_poc() {
    // get enough epochs to activate a big skiplist.
    let (serv, _, mut aud) = seed_server(65_537).await;
    let start_dig = serv.get_epoch_hash().await.unwrap();
    let vrf_pk = serv.get_public_key().await.unwrap();

    // generate Put and Get proofs, both against the same dig.
    let (end_dig, uid, hist_proof, lookup_proof) = serv.marker_attack().await;

    // Audit, just to make sure that passes as well.
    let aud_proof = serv
        .audit(start_dig.epoch(), end_dig.epoch())
        .await
        .unwrap();
    aud.audit(vec![start_dig.hash(), end_dig.hash()], aud_proof)
        .await
        .unwrap();

    key_history_verify::<TC>(
        vrf_pk.as_bytes(),
        end_dig.hash(),
        end_dig.epoch(),
        uid.clone(),
        hist_proof.clone(),
        HistoryVerificationParams::Default {
            history_params: HistoryParams::MostRecent(1),
        },
        None,
    )
    .unwrap();

    lookup_verify::<TC>(
        vrf_pk.as_bytes(),
        end_dig.hash(),
        end_dig.epoch(),
        uid.clone(),
        lookup_proof.clone(),
    )
    .unwrap();

    // it should not be possible to make these contradictory proofs.
    assert!(&hist_proof.update_proofs[0].version != &lookup_proof.version);
    assert!(&hist_proof.update_proofs[0].value != &lookup_proof.value);
    println!("poc success!");
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_put_gen_ver() {
    let (serv, _, _) = seed_server(DEF_NSEED).await;
    let vrf_pk = serv.get_public_key().await.unwrap();
    let n_ops = 10_000;
    let n_warm = get_warmup(n_ops);

    let mut total_gen: Duration = Default::default();
    let mut total_ver: Duration = Default::default();
    for i in 0..n_warm + n_ops {
        if i == n_warm {
            total_gen = Default::default();
            total_ver = Default::default();
        }
        let l = mk_rand_label();
        let elem = vec![(l.clone(), mk_rand_val())];

        let t0 = Instant::now();
        serv.publish(elem).await.unwrap();
        let (p, dig) = serv
            .key_history(&l, HistoryParams::MostRecent(1))
            .await
            .unwrap();

        let t1 = Instant::now();
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
        let t2 = Instant::now();

        total_gen += t1 - t0;
        total_ver += t2 - t1;
    }

    let m0 = total_gen.as_micros() as f64 / n_ops as f64;
    let m1 = total_gen.as_millis() as f64;
    let m2 = total_ver.as_micros() as f64 / n_ops as f64;
    let m3 = total_ver.as_millis() as f64;
    report(
        "bench_put_gen_ver".into(),
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
async fn bench_put_batch() {
    let cfgs = vec![
        (1, 50_000),
        (2, 50_000),
        (5, 10_000),
        (10, 10_000),
        (20, 10_000),
        (50, 5_000),
        (100, 3_000),
        (200, 1_500),
        (500, 500),
        (1_000, 300),
    ];
    for (batch_sz, n_batches) in cfgs.into_iter() {
        let total = put_batch_helper(batch_sz, n_batches).await;
        let m0 = total.as_micros() as f64 / n_batches as f64;
        let m1 = total.as_millis() as f64;
        report(
            "bench_put_batch".into(),
            batch_sz,
            &[
                &Metric {
                    n: m0,
                    unit: "op/s".into(),
                },
                &Metric {
                    n: m1,
                    unit: "total(ms)".into(),
                },
            ],
        );
    }
}

async fn put_batch_helper(batch_sz: i32, n_batches: i32) -> Duration {
    let (serv, _, _) = seed_server(DEF_NSEED).await;
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
    start.elapsed()
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_put_size() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let (p, dig) = serv
        .key_history(&labels[0], HistoryParams::MostRecent(1))
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
async fn bench_get_gen_ver() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let vrf_pk = serv.get_public_key().await.unwrap().to_bytes();
    let n_ops = 10_000;
    let n_warm = get_warmup(n_ops);
    let mut total_gen: Duration = Default::default();
    let mut total_ver: Duration = Default::default();

    for i in 0..n_warm + n_ops {
        if i == n_warm {
            total_gen = Default::default();
            total_ver = Default::default();
        }

        let l = labels.iter().choose(&mut rand::thread_rng()).unwrap();

        let t0 = Instant::now();
        let (p, dig) = serv.lookup(l.clone()).await.unwrap();
        if p.version as i32 != 1 {
            panic!("wrong version");
        }

        let t1 = Instant::now();
        lookup_verify::<TC>(&vrf_pk, dig.hash(), dig.epoch(), l.clone(), p).unwrap();
        let t2 = Instant::now();

        total_gen += t1 - t0;
        total_ver += t2 - t1;
    }

    let m0 = total_gen.as_micros() as f64 / n_ops as f64;
    let m1 = total_gen.as_millis() as f64;
    let m2 = total_ver.as_micros() as f64 / n_ops as f64;
    let m3 = total_ver.as_millis() as f64;
    report(
        "bench_get_gen_ver".into(),
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
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let (p, dig) = serv.lookup(labels[0].clone()).await.unwrap();
    if p.version != 1 {
        panic!("wrong version");
    }
    let pb = bincode::serialize(&p).unwrap();
    let digb = bincode::serialize(&dig).unwrap();
    let sz = (pb.len() + digb.len()) as f64;
    report(
        "bench_get_size".into(),
        1,
        &[&Metric {
            n: sz,
            unit: "B".into(),
        }],
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bench_selfmon_gen_ver() {
    let (serv, labels, _) = seed_server(DEF_NSEED).await;
    let vrf_pk = serv.get_public_key().await.unwrap().to_bytes();
    let n_ops = 20_000;
    let n_warm = get_warmup(n_ops);
    let mut total_gen: Duration = Default::default();
    let mut total_ver: Duration = Default::default();

    for i in 0..n_warm + n_ops {
        if i == n_warm {
            total_gen = Default::default();
            total_ver = Default::default();
        }

        let l = labels.iter().choose(&mut rand::thread_rng()).unwrap();

        let t0 = Instant::now();
        let (p, dig) = serv
            .key_history(&l, HistoryParams::MostRecent(0))
            .await
            .unwrap();

        let t1 = Instant::now();
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
        let t2 = Instant::now();

        total_gen += t1 - t0;
        total_ver += t2 - t1;
    }

    let m0 = total_gen.as_micros() as f64 / n_ops as f64;
    let m1 = total_gen.as_millis() as f64;
    let m2 = total_ver.as_micros() as f64 / n_ops as f64;
    let m3 = total_ver.as_millis() as f64;
    report(
        "bench_selfmon_gen_ver".into(),
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
    let (p, dig) = serv
        .key_history(&labels[0], HistoryParams::MostRecent(0))
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
async fn bench_audit_scale() {
    let cfgs = vec![
        (1, 50_000),
        (2, 50_000),
        (5, 10_000),
        (10, 10_000),
        (20, 10_000),
        (50, 5_000),
        (100, 3_000),
        (200, 1_500),
        (500, 500),
        (1_000, 300),
    ];
    for (batch_sz, n_batches) in cfgs.into_iter() {
        audit_scale_helper(batch_sz, n_batches).await;
    }
}

async fn audit_scale_helper(batch_sz: i32, n_batches: i32) {
    let (serv, _, mut aud) = seed_server(DEF_NSEED).await;
    let n_warm = get_warmup(n_batches);

    let mut total_gen: Duration = Default::default();
    let mut total_ver: Duration = Default::default();
    for i in 0..n_warm + n_batches {
        if i == n_warm {
            total_gen = Default::default();
            total_ver = Default::default();
        }
        let start_dig = serv.get_epoch_hash().await.unwrap();
        let new_els: Vec<(AkdLabel, AkdValue)> = (0..batch_sz)
            .map(|_| (mk_rand_label(), mk_rand_val()))
            .collect();
        let end_dig = serv.publish(new_els).await.unwrap();

        let t0 = Instant::now();
        let p = serv
            .audit(start_dig.epoch(), end_dig.epoch())
            .await
            .unwrap();

        let t1 = Instant::now();
        aud.audit(vec![start_dig.hash(), end_dig.hash()], p)
            .await
            .unwrap();
        let t2 = Instant::now();

        total_gen += t1 - t0;
        total_ver += t2 - t1;
    }

    let m0 = total_gen.as_micros() as f64 / n_batches as f64;
    let m1 = total_gen.as_millis() as f64;
    let m2 = total_ver.as_micros() as f64 / n_batches as f64;
    let m3 = total_ver.as_millis() as f64;
    report(
        "bench_audit_scale".into(),
        batch_sz,
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
async fn bench_audit_size() {
    let (serv, _, _) = seed_server(DEF_NSEED).await;
    let start_dig = serv.get_epoch_hash().await.unwrap();
    let elem = vec![(mk_rand_label(), mk_rand_val())];
    let end_dig = serv.publish(elem).await.unwrap();
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
async fn bench_serv_mem() {
    let db = AsyncInMemoryDatabase::new();
    let store = StorageManager::new_no_cache(db);
    let vrf = VRF {};
    let serv = Directory::<TC, _, _>::new(store, vrf, PAR_CFG)
        .await
        .unwrap();
    let n_total = 500_000_000;
    let n_measure = 1_000_000;

    let mut sys_info = sysinfo::System::new();
    let pid = sysinfo::get_current_pid().unwrap();
    sys_info.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);

    for i in (0..n_total).step_by(n_measure) {
        sys_info.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        let mb = sys_info.process(pid).unwrap().memory() as f64 / 1_000_000.0;
        report(
            "bench_serv_mem".into(),
            i as i32,
            &[&Metric {
                n: mb,
                unit: "MB".into(),
            }],
        );

        let work: Vec<(AkdLabel, AkdValue)> = (0..n_measure)
            .map(|_| (mk_rand_label(), mk_rand_val()))
            .collect();
        serv.publish(work).await.unwrap();
    }
}

async fn seed_server(
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
    if n_seed <= n_ep {
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

fn mk_rand_label() -> AkdLabel {
    // 8 bytes to match pav uint64 uid.
    let mut b = vec![0; 8];
    rand::thread_rng().fill_bytes(&mut b);
    AkdLabel(b)
}

fn mk_rand_val() -> AkdValue {
    // 32 bytes for ed25519 pk.
    let mut b = vec![0; 32];
    rand::thread_rng().fill_bytes(&mut b);
    AkdValue(b)
}

fn get_warmup(n_ops: i32) -> i32 {
    return (n_ops as f64 * 0.1) as i32;
}

#[derive(Debug, Clone, Copy)]
struct StartEnd {
    start: Instant,
    end: Instant,
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
