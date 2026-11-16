//! Unity AnimationCurve evaluation.
//!
//! Unity stores only keyframes. Between two keys it draws a cubic Hermite spline from the
//! left key's out slope and the right key's in slope. An infinite slope makes a step.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub time: f64,
    pub value: f64,
    pub in_slope: f64,
    pub out_slope: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Curve {
    pub keys: Vec<Key>,
}

impl Curve {
    pub fn new(mut keys: Vec<Key>) -> Self {
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        Self { keys }
    }

    pub fn end_time(&self) -> f64 {
        self.keys.last().map_or(0., |k| k.time)
    }

    /// The value at `time`, held flat before the first key and after the last.
    pub fn evaluate(&self, time: f64) -> f64 {
        let keys = &self.keys;
        let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
            return 0.;
        };
        if time <= first.time {
            return first.value;
        }
        if time >= last.time {
            return last.value;
        }
        let next = keys.partition_point(|k| k.time <= time);
        segment(&keys[next - 1], &keys[next], time)
    }
}

fn segment(a: &Key, b: &Key, time: f64) -> f64 {
    let dt = b.time - a.time;
    if dt <= 0. || !a.out_slope.is_finite() || !b.in_slope.is_finite() {
        return a.value;
    }
    let s = (time - a.time) / dt;
    let (s2, s3) = (s * s, s * s * s);
    (2. * s3 - 3. * s2 + 1.) * a.value
        + (s3 - 2. * s2 + s) * a.out_slope * dt
        + (-2. * s3 + 3. * s2) * b.value
        + (s3 - s2) * b.in_slope * dt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(time: f64, value: f64, slope: f64) -> Key {
        Key { time, value, in_slope: slope, out_slope: slope }
    }

    #[test]
    fn holds_before_first_and_after_last_key() {
        let curve = Curve::new(vec![key(1., 10., 0.), key(2., 20., 0.)]);
        assert_eq!(curve.evaluate(0.), 10.);
        assert_eq!(curve.evaluate(5.), 20.);
    }

    #[test]
    fn linear_slopes_give_a_straight_line() {
        let curve = Curve::new(vec![key(0., 0., 100.), key(1., 100., 100.)]);
        assert!((curve.evaluate(0.25) - 25.).abs() < 1e-9);
        assert!((curve.evaluate(0.5) - 50.).abs() < 1e-9);
    }

    #[test]
    fn flat_slopes_ease_in_and_out() {
        let curve = Curve::new(vec![key(0., 0., 0.), key(1., 100., 0.)]);
        assert!((curve.evaluate(0.5) - 50.).abs() < 1e-9);
        assert!((curve.evaluate(0.25) - 15.625).abs() < 1e-9);
    }

    #[test]
    fn infinite_slope_steps() {
        let curve = Curve::new(vec![
            Key { time: 0., value: 0., in_slope: 0., out_slope: f64::INFINITY },
            key(1., 100., 0.),
        ]);
        assert_eq!(curve.evaluate(0.99), 0.);
        assert_eq!(curve.evaluate(1.), 100.);
    }

    #[test]
    fn exact_key_time_returns_the_key_value() {
        let curve = Curve::new(vec![key(0., 0., 3.), key(1., 7., -2.), key(2., 4., 1.)]);
        assert_eq!(curve.evaluate(1.), 7.);
    }
}
