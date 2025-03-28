use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn go_test() {
    let mut parser = Parser::new();

    let code = r#"
        public void Function() {}
        namespace Tests {
            public class TestClass {
                public static void Foo() {}
                public record Record(string: Foo)
                public void Foobar() {}
                public enum Enum {
                    EnumEntity,
                    AnotherEnumEntity,
                }
                interface IInterface {
                    void Foo();
                }
            }
        }
        "#;

    let tags = parser.parse(
        &code.as_bytes().to_vec(),
        PathBuf::from("main.cs").to_str().unwrap(),
        "cs",
    );

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("Function"),
            file_name: String::from("main.cs"),
            address: String::from("/^        public void Function() {}$/;\"\t"),
        },
        Tag {
            name: String::from("Tests"),
            file_name: String::from("main.cs"),
            address: String::from("/^        namespace Tests {$/;\"\t"),
        },
        Tag {
            name: String::from("TestClass"),
            file_name: String::from("main.cs"),
            address: String::from("/^            public class TestClass {$/;\"\t"),
        },
        Tag {
            name: String::from("Foo"),
            file_name: String::from("main.cs"),
            address: String::from("/^                public static void Foo() {}$/;\"\t"),
        },
        Tag {
            name: String::from("Record"),
            file_name: String::from("main.cs"),
            address: String::from("/^                public record Record(string: Foo)$/;\"\t"),
        },
        Tag {
            name: String::from("Enum"),
            file_name: String::from("main.cs"),
            address: String::from("/^                public enum Enum {$/;\"\t"),
        },
    ];

    assert_eq!(tags, expected_tags);
}
