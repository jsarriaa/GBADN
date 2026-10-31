//! Writes the demo cartridge as FASTA: `cargo run -p gbadn-codec --example make_demo -- out.fasta`

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: make_demo <out.fasta>");
    let encoded = gbadn_codec::encode(&gbadn_codec::demo::rom()).expect("demo ROM encodes");
    std::fs::write(&path, encoded.fasta).expect("output is writable");
}
