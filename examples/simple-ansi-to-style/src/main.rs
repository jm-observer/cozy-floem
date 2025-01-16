use ansi_to_style::{index_to_rgb, parse_byte};

fn main() {
    // let input: &[u8] = &[
    //     27, 91, 49, 109, 27, 91, 51, 50, 109, 32, 32, 32, 67, 111,
    // 109, 112, 105, 108, 105, 110,     103, 27, 91, 48, 109, 32,
    // 112, 114, 111, 99, 45, 109, 97, 99, 114, 111, 50, 32, 118, 49,
    //     46, 48, 46, 57, 50,
    // ];
    // let output = parse_byte(input);
    // println!("Parsed Text: {:?}", output.text);
    // println!("Styles: {:?}", output.styles);

    let input: &[u8] = &[
        27, 91, 48, 109, 27, 91, 49, 109, 27, 91, 51, 56, 59, 53, 59,
        49, 49, 109, 119, 97, 114, 110, 105, 110, 103, 27, 91, 48,
        109, 27, 91, 48, 109, 27, 91, 49, 109, 27, 91, 51, 56, 59,
        53, 59, 49, 53, 109, 58, 32, 117, 110, 117, 115, 101, 100,
        32, 118, 97, 114, 105, 97, 98, 108, 101, 58, 32, 96, 105,
        110, 112, 117, 116, 96, 27, 91, 48, 109, 10, 27, 91, 48, 109,
        32, 27, 91, 48, 109, 27, 91, 48, 109, 27, 91, 49, 109, 27,
        91, 51, 56, 59, 53, 59, 49, 52, 109, 45, 45, 62, 32, 27, 91,
        48, 109, 27, 91, 48, 109, 115, 114, 99, 47, 109, 97, 105,
        110, 46, 114, 115, 58, 50, 58, 57, 27, 91, 48, 109, 10, 27,
        91, 48, 109, 32, 32, 27, 91, 48, 109, 27, 91, 48, 109, 27,
        91, 49, 109, 27, 91, 51, 56, 59, 53, 59, 49, 52, 109, 124,
        27, 91, 48, 109, 10, 27, 91, 48, 109, 27, 91, 49, 109, 27,
        91, 51, 56, 59, 53, 59, 49, 52, 109, 50, 27, 91, 48, 109, 27,
        91, 48, 109, 32, 27, 91, 48, 109, 27, 91, 48, 109, 27, 91,
        49, 109, 27, 91, 51, 56, 59, 53, 59, 49, 52, 109, 124, 27,
        91, 48, 109, 27, 91, 48, 109, 32, 27, 91, 48, 109, 27, 91,
        48, 109, 32, 32, 32, 32, 108, 101, 116, 32, 105, 110, 112,
        117, 116, 32, 61, 27, 91, 48, 109, 10, 27, 91, 48, 109, 32,
        32, 27, 91, 48, 109, 27, 91, 48, 109, 27, 91, 49, 109, 27,
        91, 51, 56, 59, 53, 59, 49, 52, 109, 124, 27, 91, 48, 109,
        27, 91, 48, 109, 32, 32, 32, 32, 32, 32, 32, 32, 32, 27, 91,
        48, 109, 27, 91, 48, 109, 27, 91, 49, 109, 27, 91, 51, 56,
        59, 53, 59, 49, 49, 109, 94, 94, 94, 94, 94, 27, 91, 48, 109,
        27, 91, 48, 109, 32, 27, 91, 48, 109, 27, 91, 48, 109, 27,
        91, 49, 109, 27, 91, 51, 56, 59, 53, 59, 49, 49, 109, 104,
        101, 108, 112, 58, 32, 105, 102, 32, 116, 104, 105, 115, 32,
        105, 115, 32, 105, 110, 116, 101, 110, 116, 105, 111, 110,
        97, 108, 44, 32, 112, 114, 101, 102, 105, 120, 32, 105, 116,
        32, 119, 105, 116, 104, 32, 97, 110, 32, 117, 110, 100, 101,
        114, 115, 99, 111, 114, 101, 58, 32, 96, 95, 105, 110, 112,
        117, 116, 96, 27, 91, 48, 109, 10, 27, 91, 48, 109, 32, 32,
        27, 91, 48, 109, 27, 91, 48, 109, 27, 91, 49, 109, 27, 91,
        51, 56, 59, 53, 59, 49, 52, 109, 124, 27, 91, 48, 109, 10,
        27, 91, 48, 109, 32, 32, 27, 91, 48, 109, 27, 91, 48, 109,
        27, 91, 49, 109, 27, 91, 51, 56, 59, 53, 59, 49, 52, 109, 61,
        32, 27, 91, 48, 109, 27, 91, 48, 109, 27, 91, 49, 109, 27,
        91, 51, 56, 59, 53, 59, 49, 53, 109, 110, 111, 116, 101, 27,
        91, 48, 109, 27, 91, 48, 109, 58, 32, 96, 35, 91, 119, 97,
        114, 110, 40, 117, 110, 117, 115, 101, 100, 95, 118, 97, 114,
        105, 97, 98, 108, 101, 115, 41, 93, 96, 32, 111, 110, 32, 98,
        121, 32, 100, 101, 102, 97, 117, 108, 116, 27, 91, 48, 109,
        10, 10
    ];
    /*
    warning: unused variable: `input`
    --> src/main.rs:2:9
        |
        2 |     let input =
    |         ^^^^^ help: if this is intentional, prefix it with an underscore: `_input`
    |
    = note: `#[warn(unused_variables)]` on by default
        */
    //     "\u{1b}[0m\u{1b}[1m\u{1b}[38;5;11mwarning\u{1b}[0m\
    // u{1b}[0m\u{1b}[1m\u{1b}[38;5;15m: unused variable:
    // `input`\u{1b}[0m\n \u{1b}[0m \u{1b}[0m\u{1b}[0m\u{1b}[1m\
    // u{1b}[38;5;14m-->
    // \u{1b}[0m\u{1b}[0msrc/main.rs:2:9\u{1b}[0m\n\u{1b}[0m
    // \u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[38;5;14m|\u{1b}[0m\n\
    // u{1b}[0m\u{1b}[1m\ u{1b}[38;5;14m2\u{1b}[0m\u{1b}[0m
    // \u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[38;5;14m|\u{1b}[0m\u{1b}[0m
    // \u{1b}[0m\u{1b}[0m    let input =\u{1b}[0m\n\u{1b}[0m
    // \u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[38;5;14m|\u{1b}[0m\u{1b}[0m
    // \u{1b}[0 // m\u{1b}[0m\u{1b}[1m\u{1b}[38;5;11m^^^^^\
    // u{1b}[0m\u{1b}[0m
    // \u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[38;5;11mhelp: if this is
    // intentional, prefix it with an underscore:
    // `_input`\u{1b}[0m\n\u{1b}[0m
    // \u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[38;5;14m|\u{1b}[0m\n\u{1b}[0m
    // \u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[38;5;14m=
    // \u{1b}[0m\u{1b}[0m\u{1b}[1m\u{1b}[38;5;15mnote\u{1b}[0m\
    // u{1b}[0m: `#[warn(unused_variables)]` on by
    // default\u{1b}[0m\n\n"
    let content = String::from_utf8(input.to_vec()).unwrap();
    let output = parse_byte(input);
    println!("content: {:?}", content);
    println!("Parsed Text: {}", output.text);
    println!("Styles: {:?}", output.styles);

    let index_color = [
        (124, [153, 0, 0]),
        (125, [153, 0, 51]),
        (126, [153, 0, 102]),
        (127, [153, 0, 153]),
        (128, [153, 0, 204]),
        (129, [153, 0, 255]),
        (130, [153, 51, 0]),
        (131, [153, 51, 51]),
        (132, [153, 51, 102]),
        (133, [153, 51, 153]),
        (134, [153, 51, 204]),
        (135, [153, 51, 255])
    ];

    for (index, rgb) in index_color.into_iter() {
        assert_eq!(index_to_rgb(index), rgb)
    }

    // ??
    // let index_color = [(244, [128, 128, 128]),
    // (245, [138, 138, 138]),
    // (246, [149, 149, 149]),
    // (247, [160, 160, 160]),
    // (248, [170, 170, 170]),
    // (249, [181, 181, 181]),
    // (250, [192, 192, 192]),
    // (251, [202, 202, 202]),
    // (252, [213, 213, 213]),
    // (253, [224, 224, 224]),
    // (254, [234, 234, 234]),
    // (255, [245, 245, 245])];
    // for (index, rgb) in index_color.into_iter() {
    //     assert_eq!(index_to_rgb(index), rgb)
    // }
}
