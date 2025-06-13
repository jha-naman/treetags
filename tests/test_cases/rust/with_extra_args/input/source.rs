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

