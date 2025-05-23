use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn ruby_test() {
    let mut parser = Parser::new();

    let code = r#"
        class Foo
        end

        module Bar < Object
            def self.foo
            end

            def baz
            end
        end
        "#;

    let tags = parser.generate_by_tag_query(
        &code.as_bytes().to_vec(),
        PathBuf::from("main.rb").to_str().unwrap(),
        "rb",
    );

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("Foo"),
            file_name: String::from("main.rb"),
            address: String::from("/^        class Foo$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("Bar"),
            file_name: String::from("main.rb"),
            address: String::from("/^        module Bar < Object$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("foo"),
            file_name: String::from("main.rb"),
            address: String::from("/^            def self.foo$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("baz"),
            file_name: String::from("main.rb"),
            address: String::from("/^            def baz$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
    ];

    assert_eq!(tags, expected_tags);
}
