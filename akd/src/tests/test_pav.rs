// Copyright (c) Meta Platforms, Inc. and affiliates.
//
// This source code is dual-licensed under either the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree or the Apache
// License, Version 2.0 found in the LICENSE-APACHE file in the root directory
// of this source tree. You may select, at your option, one of the above-listed licenses.

use std::time::Instant;

use akd_core::{hash::EMPTY_DIGEST, AzksElement, AzksValue, NodeLabel};
use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::{
    append_only_zks::{AzksParallelismConfig, InsertMode},
    errors::AkdError,
    storage::{manager::StorageManager, memory::AsyncInMemoryDatabase},
    Azks,
};
use akd_core::WhatsAppV1Configuration;

const NSEED: usize = 1_000_000;

#[test]
fn test_bench_prove() -> Result<(), AkdError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();
    let mut rng = StdRng::seed_from_u64(42);
    let db = AsyncInMemoryDatabase::new();
    let store = StorageManager::new_no_cache(db);

    let mut tr = rt
        .block_on(Azks::new::<WhatsAppV1Configuration, _>(&store))
        .unwrap();
    let seed = gen_rand_azks_elems(NSEED, &mut rng);
    rt.block_on(tr.batch_insert_nodes::<WhatsAppV1Configuration, _>(
        &store,
        seed,
        InsertMode::Directory,
        AzksParallelismConfig::disabled(),
    ))
    .unwrap();

    const NOPS: usize = 1_000_000;
    let mut label = [0; 32];

    let start = Instant::now();
    for _ in 0..NOPS {
        rng.fill(&mut label);
        // TODO: there's alloc here.
        let l = NodeLabel {
            label_val: label,
            label_len: 256,
        };
        rt.block_on(tr.get_non_membership_proof::<WhatsAppV1Configuration, _>(&store, l))
            .unwrap();
    }
    let total = start.elapsed();

    println!("nOps: {}", NOPS);
    let m0 = (total.as_nanos() as f64) / (NOPS as f64);
    println!("ns/op: {}", m0);
    println!("ms: {}", total.as_millis());

    Ok(())
}

fn gen_rand_azks_elems(num_nodes: usize, rng: &mut StdRng) -> Vec<AzksElement> {
    (0..num_nodes)
        .map(|_| {
            let label = random_label(rng);
            let value = EMPTY_DIGEST;
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
