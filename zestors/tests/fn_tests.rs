fn fn_once_clone(fun: impl FnOnce() + Clone + Send + 'static) {
    (fun.clone())();
    fun()
}

fn fn_mut(mut fun: impl FnMut() + Send + 'static) {
    fun();
    fun()
}

fn fn_only(fun: impl Fn() + Send + 'static) {
    fun();
    fun()
}

fn fn_pointer(fun: fn()) {}

// #[test]
fn test() {
    let mut y = "hello".to_string();
    fn_once_clone(move || {
        let y = &mut y;
        println!("fn_once: {}", y);
        y.push('1');
    });

    let mut y = "hello".to_string();
    fn_mut(move || {
        let y = &mut y;
        println!("fn_mut: {}", y);
        y.push('1');
    });

    let mut y = "hello".to_string();
    fn_mut(move || {
        let mut y = (&mut y).clone();
        println!("fn_mut(clone): {}", y);
        y.push('1');
    });
}

pub fn test_accepts() {
    let x = "hello".to_string();
    let mut y = "hello".to_string();
    let z = "hello".to_string();
    fn_once_clone(move || {
        let _ = x;
        let _ = &mut y;
        let _ = &z;
        let _ = "hello".to_string();
    });

    let mut y = "hello".to_string();
    let z = "hello".to_string();
    fn_mut(move || {
        let _ = &mut y;
        let _ = &z;
        let _ = "hello".to_string();
    });

    let z = "hello".to_string();
    fn_only(move || {
        let _ = &z;
        let _ = "hello".to_string();
    });

    fn_pointer(move || {
        let _ = "hello".to_string();
    });
}