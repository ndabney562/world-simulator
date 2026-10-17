//! Measures native snapshot fill into preallocated caller storage.
#![allow(unsafe_code)]
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ws_ffi::*;
fn snapshot(c: &mut Criterion) {
    let mut g = c.benchmark_group("snapshot_fill");
    for count in [1_000, 10_000] {
        let config = WsConfig {
            abi_version: ABI_VERSION,
            struct_size: 32,
            seed: 7,
            start_us: 0,
            debug_marker_count: count,
            reserved: 0,
        };
        // SAFETY: owned handle and preallocated disjoint buffer; handle destroyed once.
        unsafe {
            let h = ws_create(&config);
            assert!(!h.is_null());
            let mut buf = vec![0; HEADER_SIZE + count as usize * RECORD_SIZE];
            let mut n = 0;
            g.bench_function(BenchmarkId::from_parameter(count), |b| {
                b.iter(|| {
                    assert_eq!(
                        ws_fill_render_snapshot(h, buf.as_mut_ptr(), buf.len(), &mut n),
                        OK
                    );
                    std::hint::black_box(&buf);
                })
            });
            ws_destroy(h);
        }
    }
    g.finish();
}
criterion_group!(benches, snapshot);
criterion_main!(benches);
