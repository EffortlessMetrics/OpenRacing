use criterion::{Criterion, black_box, criterion_group, criterion_main};
use openracing_hid_common::{ReportBuilder, ReportParser};

fn benchmark_report_parser(c: &mut Criterion) {
    let data = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x10];

    c.bench_function("ReportParser read_u8", |b| {
        let mut parser = ReportParser::new(&data);
        b.iter(|| {
            parser.reset();
            for _ in 0..10 {
                black_box(parser.read_u8().unwrap());
            }
        });
    });

    c.bench_function("ReportParser read_u16_le", |b| {
        let mut parser = ReportParser::new(&data);
        b.iter(|| {
            parser.reset();
            for _ in 0..5 {
                black_box(parser.read_u16_le().unwrap());
            }
        });
    });

    c.bench_function("ReportParser read_u32_le", |b| {
        let mut parser = ReportParser::new(&data);
        b.iter(|| {
            parser.reset();
            for _ in 0..2 {
                black_box(parser.read_u32_le().unwrap());
            }
        });
    });
}

fn benchmark_report_builder(c: &mut Criterion) {
    c.bench_function("ReportBuilder u8", |b| {
        b.iter(|| {
            let mut builder = ReportBuilder::new(0);
            for i in 0..10 {
                builder.write_u8(black_box(i as u8));
            }
            black_box(builder.into_inner());
        });
    });

    c.bench_function("ReportBuilder u16_le", |b| {
        b.iter(|| {
            let mut builder = ReportBuilder::new(0);
            for i in 0..5 {
                builder.write_u16_le(black_box(i as u16));
            }
            black_box(builder.into_inner());
        });
    });

    c.bench_function("ReportBuilder u32_le", |b| {
        b.iter(|| {
            let mut builder = ReportBuilder::new(0);
            for i in 0..2 {
                builder.write_u32_le(black_box(i as u32));
            }
            black_box(builder.into_inner());
        });
    });
}

criterion_group!(benches, benchmark_report_parser, benchmark_report_builder);
criterion_main!(benches);
