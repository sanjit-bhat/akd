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
use akd_core::hash::EMPTY_DIGEST;
use akd_core::{AzksElement, AzksValue, NodeLabel};
use criterion::{BatchSize, Criterion};
use rand::distributions::Alphanumeric;
use rand::rngs::StdRng;
use rand::{Rng, RngCore, SeedableRng};

/*
bench_config!(bench_serv_put);
fn bench_serv_put<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();
    let rng = StdRng::seed_from_u64(42);
    let db = AsyncInMemoryDatabase::new();
    let vrf = HardCodedAkdVRF {};
    let store = StorageManager::new_no_cache(db);
    let dir = runtime
        .block_on(async move {
            Directory::<TC, _, _>::new(store, vrf, AzksParallelismConfig::disabled()).await
        })
        .unwrap();

    c.bench_function("bench_serv_put", move |b| {
        b.iter_batched(
            || dir.clone(),
            |dir| {
                let data = vec![(AkdLabel::from("User 0"), AkdValue::from("pk"))];
                runtime.block_on(dir.publish(data)).unwrap();
                let (proof, _) =
                    runtime
                        .block_on(dir.key_history(
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
*/

bench_config!(bench_merkle_put_no_proof);
fn bench_merkle_put_no_proof<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();
    let mut rng = StdRng::seed_from_u64(42);
    let database = AsyncInMemoryDatabase::new();
    let vrf = HardCodedAkdVRF {};
    let db = StorageManager::new_no_cache(database);
    let mut tree = runtime.block_on(Azks::new::<TC, _>(&db)).unwrap();
    let seed_data = gen_rand_azks_elems(1000, &mut rng);
    runtime
        .block_on(tree.batch_insert_nodes::<TC, _>(
            &db,
            seed_data,
            InsertMode::Directory,
            AzksParallelismConfig::disabled(),
        ))
        .unwrap();

    c.bench_function("bench_merkle_put_no_proof", move |b| {
        b.iter_batched(
            || {},
            |_| {
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

bench_config!(bench_merkle_put_with_proof);
fn bench_merkle_put_with_proof<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();
    let mut rng = StdRng::seed_from_u64(42);
    let database = AsyncInMemoryDatabase::new();
    let vrf = HardCodedAkdVRF {};
    let db = StorageManager::new_no_cache(database);
    let mut tree = runtime.block_on(Azks::new::<TC, _>(&db)).unwrap();
    let seed_data = gen_rand_azks_elems(1000, &mut rng);
    runtime
        .block_on(tree.batch_insert_nodes::<TC, _>(
            &db,
            seed_data,
            InsertMode::Directory,
            AzksParallelismConfig::disabled(),
        ))
        .unwrap();

    c.bench_function("bench_merkle_put_with_proof", move |b| {
        b.iter_batched(
            || {},
            |_| {
                let data = gen_rand_azks_elems(1, &mut rng);
                let data0 = data[0].clone();
                runtime
                    .block_on(tree.batch_insert_nodes::<TC, _>(
                        &db,
                        data,
                        InsertMode::Directory,
                        AzksParallelismConfig::disabled(),
                    ))
                    .unwrap();
                runtime
                    .block_on(tree.get_membership_proof::<TC, _>(&db, data0.label))
                    .unwrap();
            },
            BatchSize::PerIteration,
        );
    });
}

bench_config!(bench_merkle_get_memb);
fn bench_merkle_get_memb<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();
    let mut rng = StdRng::seed_from_u64(42);
    let database = AsyncInMemoryDatabase::new();
    let vrf = HardCodedAkdVRF {};
    let db = StorageManager::new_no_cache(database);
    let mut tree = runtime.block_on(Azks::new::<TC, _>(&db)).unwrap();
    let seed_data = gen_rand_azks_elems(1000, &mut rng);
    let data0 = seed_data[0].clone();
    runtime
        .block_on(tree.batch_insert_nodes::<TC, _>(
            &db,
            seed_data,
            InsertMode::Directory,
            AzksParallelismConfig::disabled(),
        ))
        .unwrap();

    c.bench_function("bench_merkle_get_memb", move |b| {
        b.iter_batched(
            || {},
            |_| {
                runtime
                    .block_on(tree.get_membership_proof::<TC, _>(&db, data0.label))
                    .unwrap();
            },
            BatchSize::PerIteration,
        );
    });
}

bench_config!(bench_merkle_get_nonmemb);
fn bench_merkle_get_nonmemb<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();
    let mut rng = StdRng::seed_from_u64(42);
    let database = AsyncInMemoryDatabase::new();
    let vrf = HardCodedAkdVRF {};
    let db = StorageManager::new_no_cache(database);
    let mut tree = runtime.block_on(Azks::new::<TC, _>(&db)).unwrap();
    let seed_data = gen_rand_azks_elems(1000, &mut rng);
    runtime
        .block_on(tree.batch_insert_nodes::<TC, _>(
            &db,
            seed_data,
            InsertMode::Directory,
            AzksParallelismConfig::disabled(),
        ))
        .unwrap();
    let query = gen_rand_azks_elems(1, &mut rng)[0];

    c.bench_function("bench_merkle_get_nonmemb", move |b| {
        b.iter_batched(
            || query.clone(),
            |query| {
                runtime
                    .block_on(tree.get_non_membership_proof::<TC, _>(&db, query.label))
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

group_config!(
    other_benches,
    /* bench_serv_put, */
    bench_merkle_put_no_proof,
    bench_merkle_put_with_proof,
    bench_merkle_get_memb,
    bench_merkle_get_nonmemb,
);

fn main() {
    #[cfg(feature = "whatsapp_v1")]
    other_benches_whatsapp_v1_config();
    Criterion::default().configure_from_args().final_summary();
}
