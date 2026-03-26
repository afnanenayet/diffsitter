interface Printable {
    toString(): string;
}

type Coordinate = {
    x: number;
    y: number;
};

class Vector implements Printable {
    constructor(public x: number, public y: number) {}

    magnitude(): number {
        return Math.sqrt(this.x * this.x + this.y * this.y);
    }

    toString(): string {
        return `(${this.x}, ${this.y})`;
    }
}

function add(a: Vector, b: Vector): Vector {
    return new Vector(a.x + b.x, a.y + b.y);
}

function main(): void {
    const v = new Vector(3, 4);
    console.log(v.magnitude());
}
