use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn cs_test() {
    let mut parser = Parser::new();

    let code = r#"
        public void Function() {}
        namespace Tests {
            public class TestClass {
                TestClass() {}
                public static void Foo() {}
                public record Record(string: Foo)
                public void Foobar() {}
                public static int count = 0;
                public enum Enum {
                    EnumEntity,
                    AnotherEnumEntity,
                }
                interface IInterface {
                    void Foo();
                }
                public static int IntMember { get; set; }
                public delegate int DelegateTest();
                public static event DelegateTest TestEvent;
            }
        }
        namespace Tests.Qualified {}
        "#;

    let tags = parser.generate_by_tag_query(
        &code.as_bytes().to_vec(),
        PathBuf::from("main.cs").to_str().unwrap(),
        "cs",
    );

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("Function"),
            file_name: String::from("main.cs"),
            address: String::from("/^        public void Function() {}$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("Tests"),
            file_name: String::from("main.cs"),
            address: String::from("/^        namespace Tests {$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("TestClass"),
            file_name: String::from("main.cs"),
            address: String::from("/^            public class TestClass {$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("TestClass"),
            file_name: String::from("main.cs"),
            address: String::from("/^                TestClass() {}$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("Foo"),
            file_name: String::from("main.cs"),
            address: String::from("/^                public static void Foo() {}$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("Record"),
            file_name: String::from("main.cs"),
            address: String::from("/^                public record Record(string: Foo)$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("count"),
            file_name: String::from("main.cs"),
            address: String::from("/^                public static int count = 0;$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("Enum"),
            file_name: String::from("main.cs"),
            address: String::from("/^                public enum Enum {$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("EnumEntity"),
            file_name: String::from("main.cs"),
            address: String::from("/^                    EnumEntity,$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("AnotherEnumEntity"),
            file_name: String::from("main.cs"),
            address: String::from("/^                    AnotherEnumEntity,$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("IInterface"),
            file_name: String::from("main.cs"),
            address: String::from("/^                interface IInterface {$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("Foo"),
            file_name: String::from("main.cs"),
            address: String::from("/^                    void Foo();$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("IntMember"),
            file_name: String::from("main.cs"),
            address: String::from(
                "/^                public static int IntMember { get; set; }$/;\"\t",
            ),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("DelegateTest"),
            file_name: String::from("main.cs"),
            address: String::from("/^                public delegate int DelegateTest();$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("TestEvent"),
            file_name: String::from("main.cs"),
            address: String::from(
                "/^                public static event DelegateTest TestEvent;$/;\"\t",
            ),
            extension_fields: None,
            kind: None,
        },
        Tag {
            name: String::from("Tests.Qualified"),
            file_name: String::from("main.cs"),
            address: String::from("/^        namespace Tests.Qualified {}$/;\"\t"),
            extension_fields: None,
            kind: None,
        },
    ];

    assert_eq!(tags, expected_tags);
}
