# Rust port

The device runtime in this repository is carrotpilot ported to Rust. The original is `carrot-wip` of `ajouatom/openpilot`.
The Python that prepares models, the existing C/C++ libraries and the Panda firmware are used as they are.

## Current state

- Not every feature of the original has been ported. Missing features and places that behave differently from the original are listed in `docs/md-upstream-port-gaps.md`, `docs/md-current-state-review-20261111.md` and `docs/md-port-handoff.md` in the repository.
- Building and booting is not driving validation. A change is confirmed only by a real drive on that commit.
- There are two branches. `dev-md` includes driver monitoring; `dev` is a test branch where driver monitoring does not work. Changes are not mixed between them.

## Getting and building on the device

At start the device compiles what changed and then runs it. There is no separate binary download.

1. With the car stopped, update `/data/openpilot` on the device to the commit you want.
2. Restart openpilot; compilation starts. Progress is shown on screen and in `/data/openpilot-build/build.log`.
3. `Build complete` in the log means it is done. It takes from a few minutes to more than ten depending on what changed, and longer on a first install.
4. Do not cut power or update again before compilation finishes. Running `sync` right after an update is the safe habit.

If the compiler is stopped for lack of memory, the build continues with fewer parallel jobs: 6, then 4, 3 and 2. `Compile stopped with 6 jobs` in the log is normal when `Build complete` follows. If every attempt fails, the screen shows the error with the end of the log and openpilot does not start. Read the same log; if a recovery service was built earlier it is reachable on port 6999.

The CPU sampling tool (`cpu-sample`) described in earlier guides has been removed.

## Phone and Jetson external model

How Jetlink runs the driving model on an external device is described in the [Jetlink guide](jetlink.md). There is no on/off setting and no Shadow mode: once ready it moves over by itself while stopped or while the wheel is held.

For a compatible host, the candidate forwards saved Wi-Fi profiles over the
private Jetlink connection and waits for host preparation before inference.
Wi-Fi passwords are excluded from status files and logs. A legacy host can
request the pinned model; its download is verified before transfer, and
preparation stops if the device goes onroad or the cable disconnects.
