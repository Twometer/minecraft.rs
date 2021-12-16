pub fn dist(a: f64, b: Option<f64>) -> f64 {
    if b.is_none() {
        0.0
    } else {
        b.unwrap() - a
    }
}
