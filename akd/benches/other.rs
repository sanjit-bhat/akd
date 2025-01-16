// Copyright (c) Meta Platforms, Inc. and affiliates.
//
// This source code is dual-licensed under either the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree or the Apache
// License, Version 2.0 found in the LICENSE-APACHE file in the root directory
// of this source tree. You may select, at your option, one of the above-listed licenses.
// run with: cargo bench --package akd -F bench

#[macro_use]
extern crate criterion;

mod common;

use akd::append_only_zks::{Azks, AzksParallelismConfig, InsertMode};
use akd::ecvrf::{HardCodedAkdVRF, VRFKeyStorage};
use akd::storage::manager::StorageManager;
use akd::storage::memory::AsyncInMemoryDatabase;
use akd::NamedConfiguration;
use akd::{AkdLabel, AkdValue, Directory};
use akd_core::{AzksElement, AzksValue, NodeLabel};
use akd_core::hash::EMPTY_DIGEST;
use criterion::{BatchSize, Criterion};
use rand::distributions::Alphanumeric;
use rand::rngs::StdRng;
use rand::{Rng, RngCore, SeedableRng};

bench_config!(bench_put);
fn bench_put<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("bench_put", move |b| {
        b.iter_batched(
            || {
                let rng = StdRng::seed_from_u64(42);
                let database = AsyncInMemoryDatabase::new();
                let vrf = HardCodedAkdVRF {};
                let db = StorageManager::new_no_cache(database);
                let directory = runtime
                    .block_on(async move {
                        Directory::<TC, _, _>::new(db, vrf, AzksParallelismConfig::disabled()).await
                    })
                    .unwrap();
                directory
            },
            |directory| {
                let data = vec![(AkdLabel::from("User 0"), AkdValue::from("pk"))];
                runtime.block_on(directory.publish(data)).unwrap();
                let (proof, _) =
                    runtime
                        .block_on(directory.key_history(
                            &AkdLabel::from("User 0"),
                            akd::HistoryParams::MostRecent(1),
                        ))
                        .unwrap();
                if proof.update_proofs.len() != 1 {
                    panic!("wrong number of proofs");
                }
            },
            BatchSize::PerIteration,
        );
    });
}

bench_config!(bench_merkle_put);
fn bench_merkle_put<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("bench_merkle_put", move |b| {
        b.iter_batched(
            || {
                let rng = StdRng::seed_from_u64(42);
                let database = AsyncInMemoryDatabase::new();
                let vrf = HardCodedAkdVRF {};
                let db = StorageManager::new_no_cache(database);
                let tree = runtime.block_on(Azks::new::<TC, _>(&db)).unwrap();
                (rng, db, tree)
            },
            |(mut rng, db, mut tree)| {
                let data = gen_rand_azks_elems(1, &mut rng);
                runtime
                    .block_on(tree.batch_insert_nodes::<TC, _>(
                        &db,
                        data,
                        InsertMode::Directory,
                        AzksParallelismConfig::disabled(),
                    ))
                    .unwrap();
            },
            BatchSize::PerIteration,
        );
    });
}

fn gen_rand_azks_elems(num_nodes: usize, rng: &mut StdRng) -> Vec<AzksElement> {
    (0..num_nodes)
        .map(|_| {
            let label = random_label(rng);
            let mut value = EMPTY_DIGEST;
            rng.fill_bytes(&mut value);
            AzksElement {
                label,
                value: AzksValue(value),
            }
        })
        .collect()
}

fn random_label(rng: &mut StdRng) -> NodeLabel {
    NodeLabel {
        label_val: rng.gen::<[u8; 32]>(),
        label_len: 256,
    }
}

group_config!(other_benches, bench_put, bench_merkle_put);

fn main() {
    #[cfg(feature = "whatsapp_v1")]
    other_benches_whatsapp_v1_config();
    Criterion::default().configure_from_args().final_summary();
}
