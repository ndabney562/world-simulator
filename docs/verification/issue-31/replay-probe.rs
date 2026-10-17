use ws_sim::{Lod, Population, SimTime, World, WorldConfig};
fn main() {
    for seed in [0, 1, 42, 9999] {
        for mixed in [false, true] {
            let mut world = World::new(WorldConfig {
                population: Population::exactly(20),
                ..WorldConfig::new(seed, SimTime::EPOCH)
            })
            .unwrap();
            for tick in 0..10_000_u32 {
                if mixed && tick % 137 == 0 {
                    let request: Vec<_> = world
                        .toy()
                        .buildings()
                        .ids()
                        .iter()
                        .enumerate()
                        .map(|(i, id)| {
                            (
                                *id,
                                if (i + (tick / 137) as usize) % 2 == 0 {
                                    Lod::L1
                                } else {
                                    Lod::L2
                                },
                            )
                        })
                        .collect();
                    world.set_lod(&request).unwrap();
                }
                world.step().unwrap();
                // state_hash already includes canonical position bits;
                // additionally check derived velocity bits, which it excludes.
                let mut velocity_hash = 0xcbf29ce484222325_u64;
                for (_, walk) in world.toy().walks().iter() {
                    for v in walk.velocity {
                        velocity_hash ^= u64::from(v.to_bits());
                        velocity_hash = velocity_hash.wrapping_mul(0x100000001b3);
                    }
                }
                println!(
                    "{seed} {mixed} {tick} {:016x} {velocity_hash:016x}",
                    world.state_hash()
                );
            }
        }
    }
}
