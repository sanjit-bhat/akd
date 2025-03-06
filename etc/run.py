import sys
import subprocess as sp
import json

# python run.py [test name, required] {samply, optional}
samply = False
if len(sys.argv) != 2:
    samply = True

p = sp.run(
    [
        "cargo",
        "test",
        "--profile",
        "benchtest",
        "-F",
        "benchtest",
        "-p",
        "akd",
        "--message-format",
        "json",
        "--no-run",
    ],
    text=True,
    capture_output=True,
)
akd_build = json.loads(p.stdout.splitlines()[-2])
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
