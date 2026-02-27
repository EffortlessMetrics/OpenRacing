c = open(r'H:\Code\Rust\OpenRacing\crates\telemetry-adapters\src\lib.rs', 'r', encoding='utf-8').read()
keywords = ['pub mod mudrunner', 'pub mod simhub', 'new_simhub_adapter', 
            'new_mudrunner_adapter', 'new_snowrunner_adapter',
            '"simhub"', '"mudrunner"', '"snowrunner"',
            'pub use mudrunner', 'pub use simhub',
            'pub mod dakar', 'pub mod flatout']
for kw in keywords:
    print(kw, '->', 'YES' if kw in c else 'NO')
print("File length:", len(c))
