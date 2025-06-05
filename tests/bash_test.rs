use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn bash_test() {
    let mut parser = Parser::new();

    let code = r#"
        function Test () {}
        AnotherTest () {}
        alias ll="ls -lh"
        cat > test.sh << EOF
            #!/bin/env bash
            echo "foo"
        EOF
    "#;

    let tags = parser.generate_by_tag_query(
        &code.as_bytes().to_vec(),
        PathBuf::from("main.sh").to_str().unwrap(),
        "sh",
    );

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("Test"),
            file_name: String::from("main.sh"),
            address: String::from("/^        function Test () {}$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("AnotherTest"),
            file_name: String::from("main.sh"),
            address: String::from("/^        AnotherTest () {}$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            // TODO: check why `#strip!` directive in query does not remove the trailing eq sign
            name: String::from("ll="),
            file_name: String::from("main.sh"),
            address: String::from("/^        alias ll=\"ls -lh\"$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("EOF"),
            file_name: String::from("main.sh"),
            address: String::from("/^        cat > test.sh << EOF$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
    ];

    assert_eq!(tags, expected_tags);
}
