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
