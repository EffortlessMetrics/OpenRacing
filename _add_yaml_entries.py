import re

def add_entry_after(content, after_key, new_entry):
    pattern = r'(  ' + re.escape(after_key) + r':.*?)(\n  [a-z])'
    match = re.search(pattern, content, re.DOTALL)
    if match:
        insert_pos = match.start(2)
        return content[:insert_pos] + '\n' + new_entry + content[insert_pos:]
    else:
        print(f'WARNING: key {after_key} not found')
        return content

gts_entry = """\
  gran_turismo_sport:
    name: "Gran Turismo Sport"
    versions:
      - version: "1.x"
        config_paths: []
        executable_patterns: []
        telemetry_method: "udp_salsa20_encrypted"
        supported_fields:
          - "rpm"
          - "speed_ms"
          - "gear"
          - "throttle"
          - "brake"
    telemetry:
      method: "udp_salsa20_encrypted"
      update_rate_hz: 60
      supports_360hz_option: false
      high_rate_update_rate_hz: null
      output_target: "0.0.0.0:33739"
      fields:
        ffb_scalar: null
        rpm: "engine_rpm"
        speed_ms: "speed_ms"
        slip_ratio: null
        gear: "gear_byte_low_nibble"
        flags: "flags_u32"
        car_id: null
        track_id: null
    status: "experimental"
    config_writer: "gran_turismo_sport"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"""

f1_manager_entry = """\
  f1_manager:
    name: "F1 Manager"
    versions:
      - version: "2022+"
        config_paths: []
        executable_patterns:
          - "F1Manager2022.exe"
          - "F1Manager2023.exe"
          - "F1Manager2024.exe"
        telemetry_method: "none"
        supported_fields: []
    telemetry:
      method: "none"
      update_rate_hz: 0
      supports_360hz_option: false
      high_rate_update_rate_hz: null
      output_target: ""
      fields:
        ffb_scalar: null
        rpm: null
        speed_ms: null
        slip_ratio: null
        gear: null
        flags: null
        car_id: null
        track_id: null
    status: "not_supported"
    config_writer: "f1_manager"
    auto_detect:
      process_names:
        - "F1Manager2022.exe"
        - "F1Manager2023.exe"
        - "F1Manager2024.exe"
      install_registry_keys: []
      install_paths: []
"""

nascar_21_entry = """\
  nascar_21:
    name: "NASCAR 21: Ignition"
    versions:
      - version: "2021.x"
        config_paths: []
        executable_patterns:
          - "NASCAR21.exe"
          - "Nascar21.exe"
        telemetry_method: "papyrus_udp"
        supported_fields:
          - "speed_ms"
          - "rpm"
          - "gear"
          - "throttle"
          - "brake"
    telemetry:
      method: "papyrus_udp"
      update_rate_hz: 36
      supports_360hz_option: false
      high_rate_update_rate_hz: null
      output_target: "127.0.0.1:5606"
      fields:
        ffb_scalar: null
        rpm: null
        speed_ms: null
        slip_ratio: null
        gear: null
        flags: null
        car_id: null
        track_id: null
    status: "experimental"
    config_writer: "nascar_21"
    auto_detect:
      process_names:
        - "NASCAR21.exe"
        - "Nascar21.exe"
      install_registry_keys: []
      install_paths: []
"""

for yaml_path in [
    r'crates/telemetry-config/src/game_support_matrix.yaml',
    r'crates/telemetry-support/src/game_support_matrix.yaml',
]:
    with open(yaml_path, 'r', encoding='utf-8') as f:
        content = f.read()

    # Check if already present
    if 'gran_turismo_sport:' not in content:
        content = add_entry_after(content, 'gran_turismo_7', gts_entry)
    if 'f1_manager:' not in content:
        content = add_entry_after(content, 'f1_native', f1_manager_entry)
    if 'nascar_21:' not in content:
        content = add_entry_after(content, 'nascar', nascar_21_entry)

    with open(yaml_path, 'w', encoding='utf-8', newline='\n') as f:
        f.write(content)
    print(f'Updated {yaml_path}')
