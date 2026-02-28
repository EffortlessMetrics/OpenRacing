import yaml

for p in ['crates/telemetry-config/src/game_support_matrix.yaml', 'crates/telemetry-support/src/game_support_matrix.yaml']:
    with open(p, encoding='utf-8-sig') as f:
        data = yaml.safe_load(f)

    for gid in ['gran_turismo_sport', 'f1_manager', 'nascar_21']:
        g = data['games'].get(gid)
        if g:
            top_method = g['telemetry']['method']
            for v in g['versions']:
                vm = v.get('telemetry_method')
                match = (vm == top_method)
                print(f'{p.split("/")[1]} {gid}: top={top_method}, version={vm}, consistent={match}')
        else:
            print(f'{p.split("/")[1]} {gid}: NOT FOUND')
