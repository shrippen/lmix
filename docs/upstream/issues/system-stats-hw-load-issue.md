Title: System stats: hardware decoder/encoder utilization and Strom's own memory

## What is missing

1. `GpuStats` reports GPU utilization, but the video engines (NVDEC, NVENC) saturate long before the GPU cores in decode-heavy flows: 32 x 1080p30 NVDEC inputs put NVDEC at 100 % while GPU utilization reads 4 %. A client cannot see why inputs drop frames.
2. `SystemStats` reports the machine's memory, not Strom's. On a long run a client cannot tell a leak in Strom from memory used elsewhere on the machine.

## Proposed direction

- `GpuStats.decoder_utilization` and `encoder_utilization` (percent) from NVML (`nvmlDeviceGetDecoderUtilization` / `EncoderUtilization`); `None` in the nvidia-smi fallback.
- `SystemStats.process_memory`: the resident memory of the Strom process (sysinfo, refreshed with the other stats about once a second).

Both optional with `serde(default)`, so older clients and servers keep parsing; `openapi.json` updated. Two new fields on existing types, no new endpoint.

## Questions for the maintainers

1. Are these fields welcome on the existing stats types, or should hardware engine load go elsewhere (per GPU it already is)?
2. AMD/Intel: no equivalent is proposed here. Leave `None`, or wait for a cross-vendor source?

## Test case

- A unit test on the NVML mapping: utilization values land in the new fields; the nvidia-smi fallback leaves them `None`.
- `process_memory` is `Some` and above zero in a running backend; the event round-trip test in `strom-types` covers the new field.
- Manual: 16+ NVDEC inputs show `decoder_utilization` far above GPU utilization.
