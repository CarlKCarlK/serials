pub struct Leds;

impl Leds {
    pub const SEG_A: u8 = 0b_0000_0001;
    pub const SEG_B: u8 = 0b_0000_0010;
    pub const SEG_C: u8 = 0b_0000_0100;
    pub const SEG_D: u8 = 0b_0000_1000;
    pub const SEG_E: u8 = 0b_0001_0000;
    pub const SEG_F: u8 = 0b_0010_0000;
    pub const SEG_G: u8 = 0b_0100_0000;
    pub const DECIMAL: u8 = 0b_1000_0000;

    pub const DIGITS: [u8; 10] = [
        0b_0011_1111, // 0
        0b_0000_0110, // 1
        0b_0101_1011, // 2
        0b_0100_1111, // 3
        0b_0110_0110, // 4
        0b_0110_1101, // 5
        0b_0111_1101, // 6
        0b_0000_0111, // 7
        0b_0111_1111, // 8
        0b_0110_1111, // 9
    ];

    pub const ASCII_TABLE: [u8; 128] = {
        let mut table = [0u8; 128];
        table[b'0' as usize] = 0b_0011_1111;
        table[b'1' as usize] = 0b_0000_0110;
        table[b'2' as usize] = 0b_0101_1011;
        table[b'3' as usize] = 0b_0100_1111;
        table[b'4' as usize] = 0b_0110_0110;
        table[b'5' as usize] = 0b_0110_1101;
        table[b'6' as usize] = 0b_0111_1101;
        table[b'7' as usize] = 0b_0000_0111;
        table[b'8' as usize] = 0b_0111_1111;
        table[b'9' as usize] = 0b_0110_1111;
        table[b'A' as usize] = 0b_0111_0111;
        table[b'b' as usize] = 0b_0111_1100;
        table[b'C' as usize] = 0b_0011_1001;
        table[b'd' as usize] = 0b_0101_1110;
        table[b'E' as usize] = 0b_0111_1001;
        table[b'F' as usize] = 0b_0111_0001;
        table[b'a' as usize] = 0b_0111_0111;
        table[b'c' as usize] = 0b_0011_1001;
        table[b'e' as usize] = 0b_0111_1001;
        table[b'f' as usize] = 0b_0111_0001;
        table[b'-' as usize] = 0b_0100_0000;
        table[b' ' as usize] = 0;
        table
    };
}
