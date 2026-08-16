// Dump decoded (raw ABGR2222) tiles from a pack, one file per tile, so
// alternative encodings can be measured against the same pixels.
use slippypack_core::format::reader::RawtilesReader;
use slippypack_core::format::rle8;
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    let reader = RawtilesReader::open(&bytes).unwrap();
    let meta = reader.metadata();
    let dim = meta.tile_dim_px as usize;
    let raw_len = dim * dim;
    let outdir = &args[2];
    std::fs::create_dir_all(outdir).unwrap();

    let mut manifest = std::fs::File::create(format!("{outdir}/manifest.tsv")).unwrap();
    let entries: Vec<_> = reader.tile_entries().collect();
    println!("tiles: {}, tile_dim: {dim}, raw per tile: {raw_len} bytes", entries.len());
    for (i, e) in entries.iter().enumerate() {
        let comp = reader.tile_bytes(e.z, e.x, e.y).unwrap();
        let raw = rle8::decode(comp, raw_len).unwrap();
        std::fs::write(format!("{outdir}/{i:06}.raw"), &raw).unwrap();
        writeln!(manifest, "{i}\t{}\t{}\t{}\t{}\t{}", e.z, e.x, e.y, comp.len(), raw.len()).unwrap();
    }
}
