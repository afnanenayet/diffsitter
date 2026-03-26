#include <stdio.h>
#include <math.h>

struct Point {
    double x;
    double y;
};

double distance(struct Point a, struct Point b) {
    double dx = a.x - b.x;
    double dy = a.y - b.y;
    return sqrt(dx * dx + dy * dy);
}

int main() {
    struct Point p = {1.0, 2.0};
    struct Point q = {4.0, 6.0};
    printf("Distance: %f\n", distance(p, q));
    return 0;
}
