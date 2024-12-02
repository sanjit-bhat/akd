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

use akd::append_only_zks::AzksParallelismConfig;
use akd::ecvrf::{HardCodedAkdVRF, VRFKeyStorage};
use akd::storage::manager::StorageManager;
use akd::storage::memory::AsyncInMemoryDatabase;
use akd::NamedConfiguration;
use akd::{AkdLabel, AkdValue, Directory};
use criterion::{BatchSize, Criterion};
use rand::distributions::Alphanumeric;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

bench_config!(bench_put);
fn bench_put<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("bench_put", move |b| {
        b.iter_batched(
            || {
                let mut rng = StdRng::seed_from_u64(42);
                let database = AsyncInMemoryDatabase::new();
                let vrf = HardCodedAkdVRF {};
                let db = StorageManager::new_no_cache(database);
                let db_clone = db.clone();
                let directory = runtime
                    .block_on(async move {
                        Directory::<TC, _, _>::new(db, vrf, AzksParallelismConfig::disabled()).await
                    })
                    .unwrap();
                (directory, db_clone)
            },
            |(directory, db)| {
                let data = vec![(AkdLabel::from("User 0"), AkdValue::from("pk"))];
                runtime.block_on(directory.publish(data)).unwrap();
                let (proof, _) = runtime.block_on(directory.key_history(&AkdLabel::from("User 0"), akd::HistoryParams::MostRecent(1))).unwrap();
                if proof.update_proofs.len() != 1 {
                    panic!("wrong number of proofs");
                }
            },
            BatchSize::PerIteration,
        );
    });
}

bench_config!(bench_vrf_prove);
fn bench_vrf_prove<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("bench_vrf_prove", move |b| {
        b.iter_batched(
            || {
                let mut rng = StdRng::seed_from_u64(42);
                let vrf = HardCodedAkdVRF {};
                let sk = runtime.block_on(vrf.get_vrf_private_key()).unwrap();
                sk
            },
            |vrf| {
                let inp = String::from("foo");
                vrf.prove(inp.as_bytes())
            },
            BatchSize::PerIteration,
        );
    });
}

bench_config!(bench_vrf_eval);
fn bench_vrf_eval<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("bench_vrf_eval", move |b| {
        b.iter_batched(
            || {
                let mut rng = StdRng::seed_from_u64(42);
                let vrf = HardCodedAkdVRF {};
                let sk = runtime.block_on(vrf.get_vrf_private_key()).unwrap();
                sk
            },
            |vrf| {
                vrf.evaluate("foo".as_bytes())
            },
            BatchSize::PerIteration,
        );
    });
}

bench_config!(bench_vrf_verify);
fn bench_vrf_verify<TC: NamedConfiguration>(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();

    c.bench_function("bench_vrf_verify", move |b| {
        b.iter_batched(
            || {
                let mut rng = StdRng::seed_from_u64(42);
                let vrf = HardCodedAkdVRF {};
                let sk = runtime.block_on(vrf.get_vrf_private_key()).unwrap();
                let pk = runtime.block_on(vrf.get_vrf_public_key()).unwrap();
                let proof = sk.prove("foo".as_bytes());
                (pk, proof)
            },
            |(pk, proof)| {
                pk.verify(&proof, "foo".as_bytes());
            },
            BatchSize::PerIteration,
        );
    });
}

group_config!(other_benches, bench_put, bench_vrf_prove, bench_vrf_eval, bench_vrf_verify);

fn main() {
    // NOTE(new_config): Add a new configuration here

    #[cfg(feature = "whatsapp_v1")]
    other_benches_whatsapp_v1_config();

    Criterion::default().configure_from_args().final_summary();
}
