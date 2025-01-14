use ansi_to_style::parse_byte;

fn main() {
    let input =
        b"\x1b[38;2;255;0;0mRed Text\x1b[0m Normal Text \x1b[48;2;0;255;0mGreen Background\x1b[0m";
    let output = parse_byte(input);
    println!("Parsed Text: {:?}", output.text);
    println!("Styles: {:?}", output.styles);

    let input: &[u8] = &[
        27, 91, 49, 109, 27, 91, 51, 50, 109, 32, 32, 32, 67, 111, 109, 112, 105, 108, 105, 110,
        103, 27, 91, 48, 109, 32, 112, 114, 111, 99, 45, 109, 97, 99, 114, 111, 50, 32, 118, 49,
        46, 48, 46, 57, 50,
    ];
    let output = parse_byte(input);
    println!("Parsed Text: {:?}", output.text);
    println!("Styles: {:?}", output.styles);
}
