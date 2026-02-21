# ARESRS DLL API Reference

This document describes how to use the exported C ABI from `ares` (`cdylib`).

## Build

```bash
cargo build --release --lib
```

Output (Windows MSVC target):

- `target\x86_64-pc-windows-msvc\release\ares.dll`

## Calling convention

- All exports use `extern "C"`.
- Bool-like inputs/outputs are `i32` (`0 = false`, non-zero = true).
- String inputs are `*const c_char` (UTF-8, null-terminated).
- Any pointer output must be valid and writable by caller.

## Core structs

```c
typedef struct ArNetworkSnapshotC {
    uint32_t network_type; // 1=Ethernet, 2=Wi-Fi, 3=Hotspot, 0=Unknown
    uint32_t band;         // 24=2.4GHz, 5=5GHz, 6=6GHz, 245=2.4/5, 0=Unknown
    uint64_t speed_mbps;
    int32_t has_speed;     // 0/1
} ArNetworkSnapshotC;

typedef struct ArSpoofReportC {
    int32_t machine_guid_changed; // 0/1
    uint32_t mac_values_changed;
    uint32_t mac_values_total;
    int32_t passed;               // 0/1
} ArSpoofReportC;

typedef struct ArCycleOptions {
    int32_t clean_and_reinstall;
    int32_t use_bootstrapper;
    int32_t prefer_bootstrapper_install;
    int32_t open_roblox_after_spoof;
    int32_t verify_network;
    int32_t verify_post;
    uint32_t network_verify_wait_ms;
    const char* bootstrapper_path;      // nullable
    const char* bootstrapper_cli_flag;  // nullable
} ArCycleOptions;
```

## ETW callback types

```c
typedef void (__cdecl *ArEtwProcessCallback)(
    int32_t event_kind, // 1=start, 2=stop
    uint32_t pid,
    int32_t exe_kind,   // 1=RobloxPlayerBeta, 2=RobloxPlayerLauncher, 3=RobloxCrashHandler
    const char* path,
    void* user_data
);

typedef void (__cdecl *ArEtwStateCallback)(
    int32_t state_kind, // 1=wait, 2=ready
    void* user_data
);
```

## Exported functions

### Process/clean/install

- `int32_t ArKillProcess(const char* process_name);`
- `void ArRunTraceCleaner(int32_t use_bootstrapper, const char* bootstrapper_path);`
- `int32_t ArInstallBootstrapper(const char* path, const char* cli_flag);`
- `int32_t ArInstallWeb(void);`

### Spoof pipeline

- `int32_t ArSpoofWMI(void);`
- `int32_t ArSpoofRegistry(void);`
- `void ArSpoofMAC(void);`
- `int32_t ArRunSpoofPipeline(int32_t verify_network, uint32_t wait_ms, int32_t verify_post, ArSpoofReportC* out_report);`
- `int32_t ArRunCycle(const ArCycleOptions* opts, ArSpoofReportC* out_report);`

### Network snapshot/verification

- `int32_t ArCaptureActiveNetworkSnapshot(ArNetworkSnapshotC* out);`
- `int32_t ArVerifyNetworkPreserved(const ArNetworkSnapshotC* before, uint32_t wait_ms);`

### Post-spoof validation handles

- `uint64_t ArCaptureSpoofStateHandle(void);`
- `int32_t ArVerifySpoofApplied(uint64_t before_handle, ArSpoofReportC* out_report);`
- `int32_t ArReleaseSpoofStateHandle(uint64_t handle);`

### ETW bridge

- `void ArSetEtwCallbacks(ArEtwProcessCallback process_cb, ArEtwStateCallback state_cb, void* user_data);`
- `uint64_t ArStartEtwCallbackBridge(void);`
- `int32_t ArStopEtwCallbackBridge(uint64_t handle);`

## Recommended usage patterns

### 1) One-shot full cycle

1. Fill `ArCycleOptions`.
2. Call `ArRunCycle(&opts, &report)`.
3. Check return value and `report.passed`.

### 2) Manual pipeline with explicit checks

1. `ArCaptureActiveNetworkSnapshot(&snap_before)` (optional)
2. `h = ArCaptureSpoofStateHandle()`
3. `ArSpoofWMI(); ArSpoofRegistry(); ArSpoofMAC();`
4. `ArVerifyNetworkPreserved(&snap_before, 12000)` (optional)
5. `ArVerifySpoofApplied(h, &report)`
6. `ArReleaseSpoofStateHandle(h)`

### 3) ETW monitoring

1. `ArSetEtwCallbacks(process_cb, state_cb, user_data)`
2. `bridge = ArStartEtwCallbackBridge()`
3. Run your app loop
4. `ArStopEtwCallbackBridge(bridge)` before shutdown

## Notes

- `ArRunCycle` uses defaults when `opts == NULL`:
  - clean/reinstall enabled
  - network verification enabled (12s)
  - post-check enabled
- `ArInstallBootstrapper` requires a valid path.
- ETW bridge runs a worker thread internally.
- Always release spoof-state handles to avoid leaks.
