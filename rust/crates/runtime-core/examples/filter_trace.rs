use openpilot_runtime_core::filters::{BounceFilter, FirstOrderFilter};

fn main() {
    println!("case\tstep\tx\trc\tdt\tinitialized\tfirst\tbounce");
    for (case, (dt, initialized)) in [
        (0.01, true),
        (0.01, false),
        (0.05, true),
        (0.05, false),
        (1.0 / 60.0, true),
        (0.1, false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut rc = 0.2;
        let mut a = FirstOrderFilter::new(2.0, rc, dt, initialized);
        let mut b = BounceFilter::new(2.0, rc, dt, initialized, 2.0);
        let mut seed = 42_u64;
        for step in 0..2000 {
            if step == 700 {
                rc = 0.6;
                a.update_alpha(rc);
                b.update_alpha(rc);
            }
            if step == 1500 {
                rc = 0.0;
                a.update_alpha(rc);
                b.update_alpha(rc);
            }
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let x = if step < 20 {
                0.0
            } else if step < 100 {
                1.0
            } else {
                (seed >> 32) as f64 / u32::MAX as f64 * 20.0 - 10.0
            };
            println!(
                "{case}\t{step}\t{x:.17}\t{rc:.17}\t{dt:.17}\t{initialized}\t{:.17}\t{:.17}",
                a.update(x),
                b.update(x)
            );
        }
    }
}
