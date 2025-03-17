use akd_core::utils::get_marker_versions;

#[test]
fn test_markers() {
    for i in 1..=32 {
        let (past, fut) = get_marker_versions(i, i, 500_000);
        println!("{}: {:?} {:?}", i, past, fut);
    }
}

#[test]
fn test_hist_intersec() {
    let max_scan = 129;
    // assume both clients use the same epoch.
    let n_epochs = 500_000;
    for put_ver in 1..=max_scan {
        let (_, put_fut) = get_marker_versions(1, put_ver, n_epochs);

        for get_ver in 1..=max_scan {
            if put_ver == get_ver {
                continue;
            }

            let (mut get_past, get_fut) = get_marker_versions(get_ver, get_ver, n_epochs);
            get_past.push(get_ver);

            if let Some(x) = get_fut.iter().min() {
                if *x <= put_ver {
                    continue;
                }
            }
            if is_intersec(&get_past, &put_fut) {
                continue;
            }

            println!(
                "put ver: {}, put fut: {:?}, get ver: {}, get past: {:?}, get fut: {:?}",
                put_ver, put_fut, get_ver, get_past, get_fut
            );
            break;
        }
    }
}

fn is_intersec(x1: &Vec<u64>, x2: &Vec<u64>) -> bool {
    for a in x1.iter() {
        if x2.contains(a) {
            return true;
        }
    }
    false
}
