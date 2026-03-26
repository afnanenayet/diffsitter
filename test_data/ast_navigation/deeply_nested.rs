mod outer {
    struct Container {
        value: i32,
    }

    impl Container {
        fn process(&self) -> i32 {
            let transform = |x: i32| {
                let inner = x * 2;
                inner + 1
            };
            transform(self.value)
        }

        fn get_value(&self) -> i32 {
            self.value
        }
    }

    fn helper() -> Container {
        Container { value: 42 }
    }
}
