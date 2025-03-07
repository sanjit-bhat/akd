import sys
import subprocess as sp
import json

# python run.py [test name, required] {samply, optional}
samply = False
if len(sys.argv) != 2:
    samply = True

build_cmd = [
    "cargo",
    "test",
    "--profile",
    "benchtest",
    "--features",
    "benchtest",
    "--no-default-features",
    "--package",
    "akd",
    "--no-run",
    "--message-format",
    "json",
]
p = sp.run(
    build_cmd,
    text=True,
    capture_output=True,
)
akd_build = json.loads(p.stdout.splitlines()[-2])
if "executable" not in akd_build:
    print("error: failed build cmd:", " ".join(build_cmd[:-2]))
    sys.exit(1)

exe = akd_build["executable"]
test_cmd = [exe, "--nocapture", "--test-threads", "1", sys.argv[1]]
if samply:
    samply_cmd = [
        "samply",
        "record",
        "--unstable-presymbolicate",
        "--save-only",
    ] + test_cmd
    sp.run(samply_cmd)
else:
    sp.run(test_cmd)
