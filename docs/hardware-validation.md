# Dongle Hardware Validation

## Build matrix

Run these commands before flashing or commissioning:

```sh
cargo test -p switcher-protocol
cargo test -p hub
cargo check -p hub --features matter
cargo check-dongle
```

For a live BLE sanity check against physical hardware, run the ignored hub test with an optional known address:

```sh
DONGLE_BLE_ADDRESS=AA:BB:CC:DD:EE:FF cargo test -p hub btleplug_client_can_read_live_dongle -- --ignored
```

## BLE-only validation

1. Flash `dongle` to the nRF52840 target.
2. Start the hub with BLE enabled and confirm it discovers the dongle by name or service UUID.
3. Verify the hub can read:
   - relay state
   - device identity
   - health status
4. Issue `On`, `Off`, and `Toggle` commands from the hub and confirm:
   - the relay output changes
   - the status LED mirrors the relay state
   - the hub receives a state notification after each command
5. Power-cycle or move the dongle out of range, then confirm the hub reports a disconnected bridge and recovers after the dongle returns.
6. Restart the hub and confirm it prefers the last persisted BLE address when reconnecting.

## Matter bridge validation

1. Start the hub with Matter enabled.
2. Commission the hub into a Matter controller.
3. Send `On` and `Off` Matter commands and confirm the physical relay changes.
4. Trigger a BLE-side state change and confirm the Matter on/off cluster updates to match.
5. Restart the hub and confirm it restores persisted identity, health, and relay state once BLE reconnects.
