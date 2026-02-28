f = r'crates\telemetry-adapters\src\forza.rs'
with open(f, 'r', encoding='utf-8') as fp:
    c = fp.read()

idx = c.find('fn make_sled_packet')
print("Found at char:", idx)
print(repr(c[idx:idx+100]))