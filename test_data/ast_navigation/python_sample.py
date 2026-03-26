MAX_RETRIES = 3

class Animal:
    def __init__(self, name, sound):
        self.name = name
        self.sound = sound

    def speak(self):
        return f"{self.name} says {self.sound}"

    def is_loud(self):
        return len(self.sound) > 5

def greet(animal):
    print(animal.speak())

def main():
    cat = Animal("Cat", "meow")
    greet(cat)
