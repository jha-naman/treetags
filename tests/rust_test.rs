use std::collections::HashMap;
use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn rust_test() {
    let mut parser = Parser::new();

    let code = r#"
mod example {
    mod nested_mod {
        mod inner {}
        pub struct NestedStruct {
            pub x: f64,
        }
    }
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    impl Point {
        pub fn new(x: f64, y: f64) -> Self {
            Point { x, y }
        }

        pub fn distance(&self, other: &Point) -> f64 {
            let dx = self.x - other.x;
            let dy = self.y - other.y;
            (dx * dx + dy * dy).sqrt()
        }
    }

    pub trait Shape {
        fn area(&self) -> f64;
        fn perimeter(&self) -> f64;
    }

    pub enum Color {
        Red,
        Green,
        Blue,
        Custom(u8, u8, u8),
    }

    pub struct Circle {
        center: Point,
        radius: f64,
        color: Color,
    }

    impl Circle {
        pub fn new(center: Point, radius: f64) -> Self {
            Circle {
                center,
                radius,
                color: Color::Blue
            }
        }
    }

    impl Shape for Circle {
        fn area(&self) -> f64 {
            std::f64::consts::PI * self.radius * self.radius
        }

        fn perimeter(&self) -> f64 {
            2.0 * std::f64::consts::PI * self.radius
        }
    }

    pub type Coordinate = (f64, f64);

    pub const PI: f64 = 3.14159265359;

    macro_rules! create_point {
        ($x:expr, $y:expr) => {
            Point::new($x, $y)
        }
    }

    pub static ORIGIN: Point = Point { x: 0.0, y: 0.0 };
}
        "#;

    let tags = parser
        .generate_by_walking(
            &code.as_bytes().to_vec(),
            PathBuf::from("src/main.rs").to_str().unwrap(),
            "rs",
        )
        .unwrap();

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("example"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^mod example {$/;\""),
            kind: Some(String::from("n")),
            extension_fields: None,
        },
        Tag {
            name: String::from("nested_mod"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    mod nested_mod {$/;\""),
            kind: Some(String::from("n")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("inner"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        mod inner {}$/;\""),
            kind: Some(String::from("n")),
            extension_fields: Some(create_hashmap(&[("module", "example::nested_mod")])),
        },
        Tag {
            name: String::from("NestedStruct"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        pub struct NestedStruct {$/;\""),
            kind: Some(String::from("s")),
            extension_fields: Some(create_hashmap(&[("module", "example::nested_mod")])),
        },
        Tag {
            name: String::from("x"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^            pub x: f64,$/;\""),
            kind: Some(String::from("m")),
            extension_fields: Some(create_hashmap(&[
                ("struct", "NestedStruct"),
                ("module", "example::nested_mod"),
            ])),
        },
        Tag {
            name: String::from("Point"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    pub struct Point {$/;\""),
            kind: Some(String::from("s")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("x"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        pub x: f64,$/;\""),
            kind: Some(String::from("m")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("struct", "Point"),
            ])),
        },
        Tag {
            name: String::from("y"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        pub y: f64,$/;\""),
            kind: Some(String::from("m")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("struct", "Point"),
            ])),
        },
        Tag {
            name: String::from("Point"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    impl Point {$/;\""),
            kind: Some(String::from("c")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("new"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        pub fn new(x: f64, y: f64) -> Self {$/;\""),
            kind: Some(String::from("P")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("implementation", "Point"),
            ])),
        },
        Tag {
            name: String::from("distance"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        pub fn distance(&self, other: &Point) -> f64 {$/;\""),
            kind: Some(String::from("P")),
            extension_fields: Some(create_hashmap(&[
                ("implementation", "Point"),
                ("module", "example"),
            ])),
        },
        Tag {
            name: String::from("Shape"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    pub trait Shape {$/;\""),
            kind: Some(String::from("i")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("area"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        fn area(&self) -> f64;$/;\""),
            kind: Some(String::from("P")),
            extension_fields: Some(create_hashmap(&[
                ("interface", "Shape"),
                ("module", "example"),
            ])),
        },
        Tag {
            name: String::from("perimeter"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        fn perimeter(&self) -> f64;$/;\""),
            kind: Some(String::from("P")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("interface", "Shape"),
            ])),
        },
        Tag {
            name: String::from("Color"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    pub enum Color {$/;\""),
            kind: Some(String::from("g")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("Red"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        Red,$/;\""),
            kind: Some(String::from("e")),
            extension_fields: Some(create_hashmap(&[("module", "example"), ("enum", "Color")])),
        },
        Tag {
            name: String::from("Green"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        Green,$/;\""),
            kind: Some(String::from("e")),
            extension_fields: Some(create_hashmap(&[("module", "example"), ("enum", "Color")])),
        },
        Tag {
            name: String::from("Blue"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        Blue,$/;\""),
            kind: Some(String::from("e")),
            extension_fields: Some(create_hashmap(&[("module", "example"), ("enum", "Color")])),
        },
        Tag {
            name: String::from("Custom"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        Custom(u8, u8, u8),$/;\""),
            kind: Some(String::from("e")),
            extension_fields: Some(create_hashmap(&[("module", "example"), ("enum", "Color")])),
        },
        Tag {
            name: String::from("Circle"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    pub struct Circle {$/;\""),
            kind: Some(String::from("s")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("center"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        center: Point,$/;\""),
            kind: Some(String::from("m")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("struct", "Circle"),
            ])),
        },
        Tag {
            name: String::from("radius"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        radius: f64,$/;\""),
            kind: Some(String::from("m")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("struct", "Circle"),
            ])),
        },
        Tag {
            name: String::from("color"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        color: Color,$/;\""),
            kind: Some(String::from("m")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("struct", "Circle"),
            ])),
        },
        Tag {
            name: String::from("Circle"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    impl Circle {$/;\""),
            kind: Some(String::from("c")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("new"),
            file_name: String::from("src/main.rs"),
            address: String::from(
                "/^        pub fn new(center: Point, radius: f64) -> Self {$/;\"",
            ),
            kind: Some(String::from("P")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("implementation", "Circle"),
            ])),
        },
        Tag {
            name: String::from("Circle"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    impl Shape for Circle {$/;\""),
            kind: Some(String::from("c")),
            extension_fields: Some(create_hashmap(&[("module", "example"), ("trait", "Shape")])),
        },
        Tag {
            name: String::from("area"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        fn area(&self) -> f64 {$/;\""),
            kind: Some(String::from("P")),
            extension_fields: Some(create_hashmap(&[
                ("module", "example"),
                ("implementation", "Circle"),
            ])),
        },
        Tag {
            name: String::from("perimeter"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^        fn perimeter(&self) -> f64 {$/;\""),
            kind: Some(String::from("P")),
            extension_fields: Some(create_hashmap(&[
                ("implementation", "Circle"),
                ("module", "example"),
            ])),
        },
        Tag {
            name: String::from("Coordinate"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    pub type Coordinate = (f64, f64);$/;\""),
            kind: Some(String::from("t")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("PI"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    pub const PI: f64 = 3.14159265359;$/;\""),
            kind: Some(String::from("C")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("create_point"),
            file_name: String::from("src/main.rs"),
            address: String::from("/^    macro_rules! create_point {$/;\""),
            kind: Some(String::from("M")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
        Tag {
            name: String::from("ORIGIN"),
            file_name: String::from("src/main.rs"),
            address: String::from(
                "/^    pub static ORIGIN: Point = Point { x: 0.0, y: 0.0 };$/;\"",
            ),
            kind: Some(String::from("v")),
            extension_fields: Some(create_hashmap(&[("module", "example")])),
        },
    ];

    assert_eq!(tags, expected_tags);
}

// Helper function to create HashMap<String, String> from a slice of tuples
fn create_hashmap(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
