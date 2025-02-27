import sys
import subprocess as sp

bench_class = sys.argv[1]
if bench_class == "serv":
    bench_tests = [
        "bench_put_one",
        "bench_put_batch",
        "bench_put_verify",
        "bench_put_size",
        "bench_get_one",
        "bench_get_scale",
        "bench_get_size_one",
        "bench_get_size_multi",
        "bench_get_verify",
        "bench_selfmon_one",
        "bench_selfmon_scale",
        "bench_selfmon_size",
        "bench_selfmon_verify",
        "bench_audit_batch",
        "bench_audit_size",
    ]
elif bench_class == "scale_time":
    bench_tests = [
        "bench_scale_time",
    ]
elif bench_class == "scale_alloc":
    print("note: remember to uncomment global alloc")
    bench_tests = [
        "bench_scale_alloc",
    ]
else:
    print("invalid bench_class:", bench_class)
    sys.exit(1)

for name in bench_tests:
    sp.run(
        [
            "cargo",
            "test",
            "--profile",
            "benchtest",
            "-F",
            "benchtest",
            "-p",
            "akd",
            "--",
            "--nocapture",
            "tests::test_pav_serv::" + name,
        ]
    )
