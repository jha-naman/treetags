package treetags

import (
	"fmt"
	assert "github.com/stretchr/testify/assert"
)

func main() {}
func foo(bar, baz string, arr []string) (error, map[string]string) {}

var x, y int
var (
	a, b int
	c map[string]string
	i interface{}
	z = "zed"
)
const foo = "foo"
const (
	bar = "bar"
)

type Alias map[string]int
type AnotherOne Alias

type Stringer interface {
String() string
}

type Point struct {
x, y int
}

func (p Point) String() string {
	return fmt.Sprintf("(%d, %d)", p.x, p.y);
}

func (f *Point) Bar(baz string) map[string]string {
	return nil
}

