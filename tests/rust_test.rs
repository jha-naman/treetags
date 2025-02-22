use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn rust_test() {
    let mut parser = Parser::new();

    let code = r#"fn main() {
            println!("Hello, world!");
        }

        struct Point<X, Y> {
            x: X,
            y: Y,
        }

        impl<X, Y> Point<X, Y> {
            fn get_x(&self) -> &X {
                &self.X
            }
        }

        enum Enum {}

        trait Trait {
            fn do_trait_stuff() -> Vec<u8>;
        }

        type FunctionPointer = fn(u32) -> u32;
        "#;

    let tags = parser.parse(
        &code.as_bytes().to_vec(),
        PathBuf::from("src/main.rs").to_str().unwrap(),
        "rs",
    );

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("main"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^fn main() {$/;\"\t"),
        },
        Tag {
            name: String::from("Point"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        struct Point<X, Y> {$/;\"\t"),
        },
        Tag {
            name: String::from("get_x"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^            fn get_x(&self) -> &X {$/;\"\t"),
        },
        Tag {
            name: String::from("Enum"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        enum Enum {}$/;\"\t"),
        },
        Tag {
            name: String::from("Trait"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        trait Trait {$/;\"\t"),
        },
        Tag {
            name: String::from("FunctionPointer"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        type FunctionPointer = fn(u32) -> u32;$/;\"\t"),
        },
    ];

    assert_eq!(tags, expected_tags);
}
