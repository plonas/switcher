# Dongle Hardware Validation

## Build matrix

Run these commands before flashing or commissioning:

```sh
cargo test -p switcher-protocol
cargo test -p hub
cargo check -p hub --features matter
cargo check-dongle
```

Create a `hub-config.json` file before running the hub. Example:

```json
{
  "dongles": [
    {
      "device_id": "0011223344556677",
      "endpoint_id": 2,
      "enabled": true,
      "preferred_ble_address": "AA:BB:CC:DD:EE:FF"
    }
  ]
}
```

For a live BLE sanity check against physical hardware, run the ignored hub test with an optional known address and optional expected device ID:

```sh
DONGLE_BLE_ADDRESS=AA:BB:CC:DD:EE:FF \
DONGLE_DEVICE_ID=0011223344556677 \
cargo test -p hub btleplug_client_can_read_live_dongle -- --ignored
```

## BLE-only validation

1. Flash `dongle` to the nRF52840 target.
2. Start the hub with BLE enabled and confirm it discovers each registered dongle by preferred address, cached address, or service UUID plus identity match.
3. Verify the hub can read:
   - relay state
   - device identity
   - health status
4. Issue `On`, `Off`, and `Toggle` commands from the hub and confirm:
   - the relay output changes
   - the status LED mirrors the relay state
   - the hub receives a state notification after each command
5. Power-cycle or move a dongle out of range, then confirm only that bridge reports disconnected and recovers after the dongle returns.
6. Restart the hub and confirm it reuses `hub-config.json`, restores cached state from `hub-state.json`, and prefers the last persisted BLE address when reconnecting.

## Matter bridge validation

1. Start the hub with Matter enabled.
2. Commission the hub into a Matter controller.
3. Confirm endpoint `1` is the aggregator and each registered dongle is exposed on its configured Matter endpoint.
4. Send `On` and `Off` Matter commands to each bridged endpoint and confirm the matching physical relay changes.
5. Trigger a BLE-side state change and confirm the corresponding Matter on/off cluster updates to match.
6. Restart the hub and confirm it restores persisted identity, health, relay state, and BLE address data once each dongle reconnects.
