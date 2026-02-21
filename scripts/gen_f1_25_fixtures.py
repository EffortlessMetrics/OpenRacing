"""Generate F1 25 binary fixture files for testing."""
import struct
import os

PACKET_FORMAT = 2025
GAME_YEAR = 25
MAJOR_VER = 1
MINOR_VER = 0
PACKET_VER = 1
NUM_CARS = 22
CAR_TELEMETRY_ENTRY_SIZE = 60
CAR_STATUS_ENTRY_SIZE = 55
HEADER_SIZE = 29


def build_header(packet_id, player_index):
    buf = bytearray()
    buf += struct.pack('<H', PACKET_FORMAT)
    buf += struct.pack('<B', GAME_YEAR)
    buf += struct.pack('<B', MAJOR_VER)
    buf += struct.pack('<B', MINOR_VER)
    buf += struct.pack('<B', PACKET_VER)
    buf += struct.pack('<B', packet_id)
    buf += struct.pack('<Q', 0xDEADBEEFCAFE0000)
    buf += struct.pack('<f', 12.345)
    buf += struct.pack('<I', 42)
    buf += struct.pack('<I', 42)
    buf += struct.pack('<B', player_index)
    buf += struct.pack('<B', 255)
    assert len(buf) == HEADER_SIZE, f"Header {len(buf)} != {HEADER_SIZE}"
    return bytes(buf)


def build_car_telemetry_entry(speed_kmh, throttle, steer, brake, clutch, gear,
                               engine_rpm, drs, tyres_pressure,
                               engine_temp, tyres_surface_temp, tyres_inner_temp):
    buf = bytearray()
    buf += struct.pack('<H', speed_kmh)
    buf += struct.pack('<f', throttle)
    buf += struct.pack('<f', steer)
    buf += struct.pack('<f', brake)
    buf += struct.pack('<B', clutch)
    buf += struct.pack('<b', gear)
    buf += struct.pack('<H', engine_rpm)
    buf += struct.pack('<B', drs)
    buf += struct.pack('<B', 65)
    buf += struct.pack('<H', 0x0FFF)
    for t in [400, 380, 350, 360]:
        buf += struct.pack('<H', t)
    for t in tyres_surface_temp:
        buf += struct.pack('<B', t)
    for t in tyres_inner_temp:
        buf += struct.pack('<B', t)
    buf += struct.pack('<H', engine_temp)
    for p in tyres_pressure:
        buf += struct.pack('<f', p)
    for _ in range(4):
        buf += struct.pack('<B', 1)
    assert len(buf) == CAR_TELEMETRY_ENTRY_SIZE, f"Telem entry {len(buf)} != {CAR_TELEMETRY_ENTRY_SIZE}"
    return bytes(buf)


def build_car_status_entry(pit_limiter, fuel_in_tank, fuel_capacity,
                            fuel_remaining_laps, max_rpm, idle_rpm,
                            drs_allowed, actual_tyre_compound, visual_tyre_compound,
                            tyre_age_laps, engine_power_ice, engine_power_mguk,
                            ers_store_energy, ers_deploy_mode):
    buf = bytearray()
    buf += struct.pack('<B', 1)              # 0  tractionControl
    buf += struct.pack('<B', 1)              # 1  antiLockBrakes
    buf += struct.pack('<B', 0)              # 2  fuelMix
    buf += struct.pack('<B', 56)             # 3  frontBrakeBias
    buf += struct.pack('<B', pit_limiter)    # 4  pitLimiterStatus
    buf += struct.pack('<f', fuel_in_tank)   # 5  fuelInTank
    buf += struct.pack('<f', fuel_capacity)  # 9  fuelCapacity
    buf += struct.pack('<f', fuel_remaining_laps)  # 13 fuelRemainingLaps
    buf += struct.pack('<H', max_rpm)        # 17 maxRPM
    buf += struct.pack('<H', idle_rpm)       # 19 idleRPM
    buf += struct.pack('<B', 8)              # 21 maxGears
    buf += struct.pack('<B', drs_allowed)    # 22 drsAllowed
    buf += struct.pack('<H', 0)              # 23 drsActivationDistance
    buf += struct.pack('<B', actual_tyre_compound)  # 25 actualTyreCompound
    buf += struct.pack('<B', visual_tyre_compound)  # 26 visualTyreCompound
    buf += struct.pack('<B', tyre_age_laps)  # 27 tyresAgeLaps
    buf += struct.pack('<b', 0)              # 28 vehicleFiaFlags (signed)
    buf += struct.pack('<f', engine_power_ice)    # 29 enginePowerICE
    buf += struct.pack('<f', engine_power_mguk)   # 33 enginePowerMGUK
    buf += struct.pack('<f', ers_store_energy)    # 37 ersStoreEnergy
    buf += struct.pack('<B', ers_deploy_mode)     # 41 ersDeployMode
    buf += struct.pack('<f', 0.0)            # 42 ersHarvestedThisLapMGUK
    buf += struct.pack('<f', 0.0)            # 46 ersHarvestedThisLapMGUH
    buf += struct.pack('<f', 0.0)            # 50 ersDeployedThisLap
    buf += struct.pack('<B', 0)              # 54 networkPaused
    assert len(buf) == CAR_STATUS_ENTRY_SIZE, f"Status entry {len(buf)} != {CAR_STATUS_ENTRY_SIZE}"
    return bytes(buf)


def main():
    out_dir = os.path.join(os.path.dirname(__file__),
                            '..', 'crates', 'service', 'tests', 'fixtures', 'f1_25')
    os.makedirs(out_dir, exist_ok=True)

    # --- CarTelemetry (packet_id=6) ---
    player_index = 0
    header = build_header(6, player_index)

    player_telem = build_car_telemetry_entry(
        speed_kmh=216,      # 216 km/h ≈ 60.0 m/s
        throttle=0.95,
        steer=-0.12,
        brake=0.0,
        clutch=0,
        gear=7,
        engine_rpm=14500,
        drs=1,
        tyres_pressure=[24.5, 24.5, 26.0, 26.0],  # RL, RR, FL, FR PSI
        engine_temp=90,
        tyres_surface_temp=[85, 83, 78, 80],
        tyres_inner_temp=[95, 92, 88, 90],
    )

    cars_data = bytearray()
    for i in range(NUM_CARS):
        cars_data += player_telem if i == player_index else bytes(CAR_TELEMETRY_ENTRY_SIZE)

    trailer = struct.pack('<BBb', 0, 0, 7)
    telem_packet = header + bytes(cars_data) + trailer
    assert len(telem_packet) == 29 + 22 * 60 + 3, f"Unexpected size {len(telem_packet)}"

    path = os.path.join(out_dir, 'car_telemetry_packet.bin')
    with open(path, 'wb') as f:
        f.write(telem_packet)
    print(f"Written {path} ({len(telem_packet)} bytes)")

    # --- CarStatus (packet_id=7) ---
    header_status = build_header(7, player_index)

    player_status = build_car_status_entry(
        pit_limiter=0,
        fuel_in_tank=42.3,
        fuel_capacity=110.0,
        fuel_remaining_laps=18.5,
        max_rpm=15100,
        idle_rpm=4000,
        drs_allowed=1,
        actual_tyre_compound=16,   # 16 = C3 (medium)
        visual_tyre_compound=16,
        tyre_age_laps=5,
        engine_power_ice=450000.0,
        engine_power_mguk=120000.0,
        ers_store_energy=3200000.0,
        ers_deploy_mode=2,
    )

    cars_status = bytearray()
    for i in range(NUM_CARS):
        cars_status += player_status if i == player_index else bytes(CAR_STATUS_ENTRY_SIZE)

    status_packet = header_status + bytes(cars_status)
    assert len(status_packet) == 29 + 22 * 55, f"Unexpected size {len(status_packet)}"

    path = os.path.join(out_dir, 'car_status_packet.bin')
    with open(path, 'wb') as f:
        f.write(status_packet)
    print(f"Written {path} ({len(status_packet)} bytes)")


if __name__ == '__main__':
    main()
