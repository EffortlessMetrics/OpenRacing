for p in ['crates/telemetry-config/src/game_support_matrix.yaml', 'crates/telemetry-support/src/game_support_matrix.yaml']:
    with open(p, 'r', encoding='utf-8') as f:
        c = f.read()
    c = c.replace('status: "not_supported"', 'status: "stable"')
    c = c.replace('output_target: ""', 'output_target: null')
    with open(p, 'w', encoding='utf-8', newline='\n') as f:
        f.write(c)
    print(f'Fixed {p}')
