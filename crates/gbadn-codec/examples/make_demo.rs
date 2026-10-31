//! Writes the demo as a cartridge: `cargo run -p gbadn-codec --example make_demo -- out.fasta`
//! A path ending in `.gba` gets the plain ROM instead, as a test input for converters.

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: make_demo <out.fasta>");
    let rom = gbadn_codec::demo::rom();
    let output = if path.ends_with(".gba") {
        rom
    } else {
        gbadn_codec::encode(&rom)
            .expect("demo ROM encodes")
            .fasta
            .into_bytes()
    };
    std::fs::write(&path, output).expect("output is writable");
}
