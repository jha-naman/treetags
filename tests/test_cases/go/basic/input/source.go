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

