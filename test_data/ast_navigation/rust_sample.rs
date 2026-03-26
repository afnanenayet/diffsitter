const MAX_SIZE: usize = 100;

type Result<T> = std::result::Result<T, String>;

struct Point {
    x: f64,
    y: f64,
}

trait Shape {
    fn area(&self) -> f64;
    fn perimeter(&self) -> f64;
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn distance_to(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

impl Shape for Point {
    fn area(&self) -> f64 {
        0.0
    }

    fn perimeter(&self) -> f64 {
        0.0
    }
}

enum Color {
    Red,
    Green,
    Blue,
}

static ORIGIN: Point = Point { x: 0.0, y: 0.0 };

fn main() {
    let p = Point::new(1.0, 2.0);
    println!("{}", p.x);
}
