# Podman sandbox: deployment requirements

The agent sandbox (`core_agent::podman`) runs every agent command in a
fresh rootless podman container (`podman run --rm`, image
`localhost/susutaku-sandbox:latest`). Where the backend process lives
determines what must be installed and which container flags are needed.

## Topology

```
┌─ macOS / Linux host ────────────────────────────────────────────┐
│ backend (host process)                                          │
│   └─ podman run (rootless)                                      │
│        └─ agent command: bash -c "<cmd>"                        │
└─────────────────────────────────────────────────────────────────┘

┌─ Docker deploy stack (docker/compose/deploy.yml) ───────────────┐
│ backend container (privileged)                                  │
│   ├─ podman (apt package, baked into the image)                 │
│   ├─ podman build at container start → sandbox image            │
│   └─ podman run (rootless, nested!)                             │
│        └─ agent command                                         │
│ docker-in-docker caveat: the "outer" runtime is Docker (or      │
│ OrbStack), the "inner" runtime is podman/crun.                  │
└─────────────────────────────────────────────────────────────────┘
```

## Bare-metal / VM backend (no outer container)

| requirement | why |
|---|---|
| `podman` on PATH | the sandbox shells out to `podman run`; it never falls back to unsandboxed execution |
| unprivileged userns enabled | rootless podman creates a user namespace (`sysctl kernel.unprivileged_userns_clone=1` on Debian/Ubuntu kernels) |
| subuid/subgid for the backend user | `/etc/subuid` + `/etc/subgid` need a range like `backend:100000:65536`, or podman falls back to a single-uid mapping with reduced isolation |
| sandbox image | `make sandbox-image` (builds `docker/sandbox/Containerfile` → `localhost/susutaku-sandbox:latest`) |
| `SUSUTAKU_SANDBOX_IMAGE` (optional) | point at a custom image/registry |

If `podman` is missing, the sandbox auto-installs it on first run
(`ensure_podman`: apt-get → dnf → apk on Linux, brew on macOS) — this
needs root or passwordless package-manager rights for the backend user.
Prefer installing it properly.

macOS note: podman needs its Linux VM — one-time `podman machine init
&& podman machine start` before the backend can run anything.

## Backend inside Docker (deploy / any compose stack)

Nesting podman inside a container hits kernel boundaries one by one.
Each symptom below was hit and fixed in `docker/compose/deploy.yml` +
`docker/backend/Dockerfile.backend`:

| # | error on first run | cause | fix |
|---|---|---|---|
| 1 | namespace/operation-not-permitted on unshare | default seccomp/apparmor profile blocks nested namespace creation | `security_opt: [seccomp=unconfined, apparmor=unconfined]` |
| 2 | `overlay is not supported over overlayfs ... mount_program required` | podman's overlay storage driver cannot live on top of Docker's overlayfs | install `fuse-overlayfs` (FUSE-based storage); `fuse3` alone is not enough |
| 3 | `/dev/fuse: No such file or directory` | containers get no FUSE device by default | `devices: [/dev/fuse:/dev/fuse]` |
| 4 | `cgroup.subtree_control ... Read-only file system` / `controller pids is not available` | the outer runtime (OrbStack, nested cgroup v2) does not delegate controllers to nested containers | containers.conf.d: `[containers] cgroups = "disabled"` (baked into the image). On a plain Docker host, `cgroup: private` + `privileged` also works |
| 5 | `/proc/sys/net/ipv4/ping_group_range ... Read-only file system` | crun writes this sysctl when setting up a rootless netns; /proc/sys is read-only without privileges | `privileged: true` on the backend service |

Net effect for the deploy stack: the backend service runs `privileged:
true` with `/dev/fuse`. That is deliberate — podman-in-container without
privileges is not supported by crun today. The blast radius stays bounded:
only the backend container is privileged; agent commands still run inside
the *inner* rootless podman jail (workspace-only writable bind, env
allow-list, `--network=none` by default, rlimits, timeout kill).

### Sandbox image lifecycle in containers

- `docker/backend/Dockerfile.backend` copies
  `docker/sandbox/Containerfile` into the image
  (`/usr/share/susutaku/sandbox/`).
- `entrypoint-backend.sh` builds `localhost/susutaku-sandbox:latest` at
  container start **only if missing** (first start takes ~1–2 min for the
  apt steps inside the build).
- The compose volume `podman-storage` (`/var/lib/containers/storage`)
  persists podman's storage, so redeploys skip the rebuild.
- nftables is installed in the image because podman *builds* use netavark
  + nftables for the build network (`podman run --network=none` does not
  need it).

### Build vs run network

- `podman build` needs a working default network (slirp4netns/netavark)
  so the Containerfile's `apt-get` can reach the Debian mirrors.
- Agent runs default to `NetworkPolicyChoice::Disabled` →
  `--network=none`: no netns setup beyond loopback, no slirp needed.
- `NetworkPolicyChoice::Enabled` gives the agent outbound internet via
  rootless slirp4netns — works on hosts; inside the deploy container it
  requires the privileged setup above.

## Verification checklist

```sh
# 1. binary + storage ok
docker exec susutaku-deploy-backend-1 podman --version

# 2. image present (built at entrypoint, persisted by the volume)
docker exec susutaku-deploy-backend-1 podman image ls | grep susutaku-sandbox

# 3. the real path: manager API run (spawn + run + transcript)
curl -s -X POST localhost:3334/api/manager/agents -H 'Content-Type: application/json' \
  -d '{"agent":"podman-test"}'
curl -s -X POST localhost:3334/api/manager/agents/podman-test/run \
  -H 'Content-Type: application/json' -d '{"cmd":"echo hi && whoami"}'
#    → {"output":"hi\nroot\n"}

# 4. clean up
curl -s -X POST localhost:3334/api/manager/agents/podman-test/finish
```

A `whoami` returning `root` is expected: rootless podman maps the
container user to uid 0 *inside the user namespace*; it has no host
privileges.
