pub fn diff_opt(a: f64, b: Option<f64>) -> f64 {
    match b {
        Some(b) => b - a,
        None => 0.0,
    }
}
