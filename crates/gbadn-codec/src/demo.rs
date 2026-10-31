//! A tiny original GBA program, so GBADN can be tried and tested without any game.
//! It fills the screen in green, and in red while any button is held.

const PROGRAM: [u32; 16] = [
    0xE3A00301, // mov  r0, #0x04000000
    0xE3A01B01, // mov  r1, #0x400
    0xE3811003, // orr  r1, r1, #3         ; bitmap mode 3, background 2 on
    0xE1C010B0, // strh r1, [r0]           ; DISPCNT
    0xE2805F4C, // add  r5, r0, #0x130     ; KEYINPUT
    0xE1D540B0, // frame: ldrh r4, [r5]
    0xE2846001, // add  r6, r4, #1         ; idle KEYINPUT is 0x3FF, so this reaches 0x400
    0xE3160B01, // tst  r6, #0x400
    0x13A04E3E, // movne r4, #0x3E0        ; green when nothing is pressed
    0x03A0401F, // moveq r4, #0x1F         ; red while any button is held
    0xE3A02406, // mov  r2, #0x06000000    ; VRAM
    0xE3A03C96, // mov  r3, #0x9600        ; 240 * 160 pixels
    0xE0C240B2, // fill: strh r4, [r2], #2
    0xE2533001, // subs r3, r3, #1
    0x1AFFFFFC, // bne  fill
    0xEAFFFFF4, // b    frame
];

/// Builds the demo cartridge image.
pub fn rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x400];
    rom[0..4].copy_from_slice(&0xEA00002Eu32.to_le_bytes()); // b 0x080000C0
    rom[0xA0..0xAC].copy_from_slice(b"GBADN DEMO\0\0");
    rom[0xAC..0xB0].copy_from_slice(b"GDNA");
    rom[0xB2] = 0x96;
    let checksum = rom[0xA0..0xBD]
        .iter()
        .fold(0u8, |sum, b| sum.wrapping_sub(*b))
        .wrapping_sub(0x19);
    rom[0xBD] = checksum;
    for (index, word) in PROGRAM.iter().enumerate() {
        let start = 0xC0 + index * 4;
        rom[start..start + 4].copy_from_slice(&word.to_le_bytes());
    }
    rom
}
