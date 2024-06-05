#![warn(clippy::all, clippy::nursery, clippy::pedantic)]

use gxhash::{HashMap as GxHashMap, HashMapExt};
use std::hash::Hash;

#[divan::bench(types = [u8, u16, u32, u64], consts = [12, 16, 32], sample_size = 1000, sample_count = 1000)]
fn insert<T, const N: usize>(bencher: divan::Bencher)
where
    T: Copy + Hash + Eq + TryFrom<u64> + std::fmt::Debug + std::marker::Sync,
    <T as TryFrom<u64>>::Error: std::fmt::Debug,
{
    bencher.with_inputs(GxHashMap::default).bench_refs(|map| {
        for i in 0..N {
            map.insert(T::try_from(i as u64).unwrap(), i % 2 == 0);
        }
    });
}

#[divan::bench(types = [u8, u16, u32, u64], consts = [12, 16, 32], sample_size = 1000, sample_count = 1000)]
fn insert_prealloc<T, const N: usize>(bencher: divan::Bencher)
where
    T: Copy + Hash + Eq + TryFrom<u64> + std::fmt::Debug + std::marker::Sync,
    <T as TryFrom<u64>>::Error: std::fmt::Debug,
{
    bencher
        .with_inputs(|| GxHashMap::with_capacity(N))
        .bench_refs(|map| {
            for i in 0..N {
                map.insert(T::try_from(i as u64).unwrap(), i % 2 == 0);
            }
        });
}

#[divan::bench(types = [u8, u16, u32, u64], consts = [12, 16, 32], sample_size = 1000, sample_count = 1000)]
fn remove<T, const N: usize>(bencher: divan::Bencher)
where
    T: Copy + Hash + Eq + TryFrom<u64> + std::marker::Sync,
    <T as TryFrom<u64>>::Error: std::fmt::Debug,
{
    bencher
        .with_inputs(|| {
            (0..N)
                .map(|x| (T::try_from(x as u64).unwrap(), x % 2 == 0))
                .collect::<GxHashMap<T, bool>>()
        })
        .bench_refs(|map| {
            for i in 0..N {
                map.remove(&T::try_from(i as u64).unwrap());
            }
        });
}

#[divan::bench(types = [u8, u16, u32, u64], consts = [12, 16, 32], sample_size = 1000, sample_count = 1000)]
fn contains_key<T, const N: usize>(bencher: divan::Bencher)
where
    T: Copy + Hash + Eq + TryFrom<u64> + std::marker::Sync,
    <T as TryFrom<u64>>::Error: std::fmt::Debug,
{
    let map: GxHashMap<T, bool> = (0..N)
        .map(|x| (T::try_from(x as u64).unwrap(), x % 2 == 0))
        .collect();
    bencher.with_inputs(|| map.clone()).bench_refs(|map| {
        for i in 0..N {
            map.contains_key(&T::try_from(i as u64).unwrap());
        }
    });
}

fn main() {
    divan::main();
}
