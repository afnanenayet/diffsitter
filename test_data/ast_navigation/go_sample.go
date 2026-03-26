package main

import "fmt"

type Point struct {
	X float64
	Y float64
}

type Stringer interface {
	String() string
}

func NewPoint(x, y float64) Point {
	return Point{X: x, Y: y}
}

func (p Point) String() string {
	return fmt.Sprintf("(%f, %f)", p.X, p.Y)
}

func main() {
	p := NewPoint(1.0, 2.0)
	fmt.Println(p.String())
}
