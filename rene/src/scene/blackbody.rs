fn black_body(lambda: &[f32], t: f32, le: &mut [f32]) {
    if t <= 0.0 {
        le.fill(0.0);
        return;
    }

    const C: f32 = 299792458.0;
    const H: f32 = 6.62606957e-34;
    const KB: f32 = 1.3806488e-23;

    for (l, le) in lambda.iter().zip(le.iter_mut()) {
        let l = *l * 1e-9;
        let lambda5 = (l * l) * (l * l) * l;

        *le = (2.0 * H * C * C) / (lambda5 * (((H * C) / (l * KB * t)) - 1.0).exp());
    }
}

fn black_body_normalized(lambda: &[f32], t: f32, le: &mut [f32]) {
    black_body(lambda, t, le);
    let lambda_max = vec![2.8977721e-3 / t * 1e9];
    let mut max_l = vec![0.0];
    black_body(&lambda_max, t, &mut max_l);

    for l in le.iter_mut() {
        *l /= max_l[0];
    }
}
