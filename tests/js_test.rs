use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn js_test() {
    let mut parser = Parser::new();

    let code = r#"
        var func = function() {};
        const fn = (foo, bar) => {};
        String.prototype.fn = function() {};
        function() {
            function inner() {};
        }();
        var o = {
            fn: () => {},
        };

        class Rectangle {
          constructor(height, width) {
            this.height = height;
            this.width = width;
          }

          area() {
            return this.height * this.width;
          }
        }
        "#;

    let tags = parser.parse(
        &code.as_bytes().to_vec(),
        PathBuf::from("main.js").to_str().unwrap(),
        "js",
    );

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("func"),
            file_name: String::from("main.js"),
            address: String::from("/^        var func = function() {};$/;\"\t"),
        },
        Tag {
            name: String::from("fn"),
            file_name: String::from("main.js"),
            address: String::from("/^        const fn = (foo, bar) => {};$/;\"\t"),
        },
        Tag {
            name: String::from("fn"),
            file_name: String::from("main.js"),
            address: String::from("/^        String.prototype.fn = function() {};$/;\"\t"),
        },
        Tag {
            name: String::from("inner"),
            file_name: String::from("main.js"),
            address: String::from("/^            function inner() {};$/;\"\t"),
        },
        Tag {
            name: String::from("fn"),
            file_name: String::from("main.js"),
            address: String::from("/^            fn: () => {},$/;\"\t"),
        },
        Tag {
            name: String::from("Rectangle"),
            file_name: String::from("main.js"),
            address: String::from("/^        class Rectangle {$/;\"\t"),
        },
        Tag {
            name: String::from("area"),
            file_name: String::from("main.js"),
            address: String::from("/^          area() {$/;\"\t"),
        },
    ];

    assert_eq!(tags, expected_tags);
}
