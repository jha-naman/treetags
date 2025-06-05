use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn python_test() {
    let mut parser = Parser::new();

    let code = r#"
        class Foo:
            def __init__(self, bar):
                self.bar = bar

            def bar(self):
                pass

        variable = [1, 2]

        def func(x, y):
            x + y
        "#;

    let tags = parser.generate_by_tag_query(
        &code.as_bytes().to_vec(),
        PathBuf::from("main.py").to_str().unwrap(),
        "py",
    );

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("Foo"),
            file_name: String::from("main.py"),
            address: String::from("/^        class Foo:$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("__init__"),
            file_name: String::from("main.py"),
            address: String::from("/^            def __init__(self, bar):$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("bar"),
            file_name: String::from("main.py"),
            address: String::from("/^            def bar(self):$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("variable"),
            file_name: String::from("main.py"),
            address: String::from("/^        variable = [1, 2]$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("func"),
            file_name: String::from("main.py"),
            address: String::from("/^        def func(x, y):$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
    ];

    assert_eq!(tags, expected_tags);
}
