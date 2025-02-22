use std::path::PathBuf;
use treetags::{Parser, Tag};

#[test]
fn go_test() {
    let mut parser = Parser::new();

    let code = r#"
            func main() {}

            type Stringer interface {
                String() string
            }

            type Point struct {
                x, y int
            }

            func (p Point) String() string {
                return fmt.Sprintf("(%d, %d)", p.x, p.y);
            }
        "#;

    let tags = parser.parse(
        &code.as_bytes().to_vec(),
        PathBuf::from("main.go").to_str().unwrap(),
        "go",
    );

    let expected_tags: Vec<Tag> = vec![
        Tag {
            name: String::from("main"),
            file_name: String::from("main.go"),
            address: String::from("/^            func main() {}$/;\"\t"),
        },
        Tag {
            name: String::from("Stringer"),
            file_name: String::from("main.go"),
            address: String::from("/^            type Stringer interface {$/;\"\t"),
        },
        Tag {
            name: String::from("Point"),
            file_name: String::from("main.go"),
            address: String::from("/^            type Point struct {$/;\"\t"),
        },
        Tag {
            name: String::from("String"),
            file_name: String::from("main.go"),
            address: String::from("/^            func (p Point) String() string {$/;\"\t"),
        },
    ];

    assert_eq!(tags, expected_tags);
}
