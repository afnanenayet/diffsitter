interface Describable {
    String describe();
}

class Animal implements Describable {
    private String name;
    private String sound;

    Animal(String name, String sound) {
        this.name = name;
        this.sound = sound;
    }

    String speak() {
        return name + " says " + sound;
    }

    public String describe() {
        return "Animal: " + name;
    }
}

class Main {
    public static void main(String[] args) {
        Animal cat = new Animal("Cat", "meow");
        System.out.println(cat.speak());
    }
}
