use std::fmt::Write;

pub struct Metric {
    pub n: f64,
    pub unit: String,
}

pub fn report(caller_name: String, n_ops: i32, metrics: &[&Metric]) {
    let mut buf = String::new();
    write!(buf, "\n{:<20}\t{:8}", caller_name, n_ops).unwrap();

    for metric in metrics {
        buf.push('\t');
        pretty_print(&mut buf, metric.n, &metric.unit).unwrap();
    }

    println!("{}", buf);
}

fn pretty_print(w: &mut dyn Write, x: f64, unit: &str) -> std::fmt::Result {
    let y = x.abs();
    if y == 0.0 || y >= 999.95 {
        write!(w, "{:10.0} {}", x, unit)
    } else if y >= 99.995 {
        write!(w, "{:12.1} {}", x, unit)
    } else if y >= 9.9995 {
        write!(w, "{:13.2} {}", x, unit)
    } else if y >= 0.99995 {
        write!(w, "{:14.3} {}", x, unit)
    } else if y >= 0.099995 {
        write!(w, "{:15.4} {}", x, unit)
    } else if y >= 0.0099995 {
        write!(w, "{:16.5} {}", x, unit)
    } else if y >= 0.00099995 {
        write!(w, "{:17.6} {}", x, unit)
    } else {
        write!(w, "{:18.7} {}", x, unit)
    }
}

// Rust port of
// https://github.com/aclements/go-moremath/blob/f10218a/stats/sample.go,
// without weighting.
pub struct Sample {
    pub xs: Vec<f64>,
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
