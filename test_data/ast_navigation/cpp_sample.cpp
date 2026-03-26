#include <string>
#include <cmath>

struct Point {
    double x;
    double y;
};

class Shape {
public:
    virtual double area() const = 0;
    virtual std::string name() const = 0;
};

class Circle : public Shape {
    double radius;
public:
    Circle(double r) : radius(r) {}

    double area() const override {
        return 3.14159 * radius * radius;
    }

    std::string name() const override {
        return "Circle";
    }
};

double distance(const Point& a, const Point& b) {
    return std::sqrt((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y));
}

int main() {
    Point p = {1.0, 2.0};
    Circle c(5.0);
    return 0;
}
