//! Rope parity check vs mlx-lm ProportionalRoPE / mx.fast.rope.
use mlx_rs::{Array, ops, ops::indexing::IndexOp, transforms::eval};

struct PropRope {
    dims: i32,
    rotated: i32,
    base: f32,
}

impl PropRope {
    fn forward(&self, x: &Array, offset: i32) -> mlx_rs::error::Result<Array> {
        let l = x.shape()[2];
        let d = x.shape()[3] as usize;
        let half = (self.dims / 2) as usize;
        let rot = (self.rotated / 2) as usize;

        let mut ang = Vec::with_capacity(l as usize * half);
        for p in 0..l as usize {
            for i in 0..half {
                let a = if i < rot {
                    let f = self.base.powf(2.0 * i as f32 / self.dims as f32);
                    (p + offset as usize) as f32 / f
                } else {
                    0.0
                };
                ang.push(a);
            }
        }
        let ang = Array::from_slice(&ang, &[l, half as i32]);
        let cos = ops::cos(&ang)?;
        let sin = ops::sin(&ang)?;
        let cos = cos.reshape(&[1, 1, l, half as i32])?.as_dtype(x.dtype())?;
        let sin = sin.reshape(&[1, 1, l, half as i32])?.as_dtype(x.dtype())?;

        let x1 = x.index((.., .., .., 0i32..half as i32));
        let x2 = x.index((.., .., .., half as i32..(2 * half) as i32));
        let o1 = x1.multiply(&cos)?.subtract(&x2.multiply(&sin)?)?;
        let o2 = x2.multiply(&cos)?.add(&x1.multiply(&sin)?)?;
        let head = mlx_rs::ops::concatenate_axis(&[&o1, &o2], 3)?;
        if 2 * half < d {
            let tail = x.index((.., .., .., (2 * half) as i32..));
            return mlx_rs::ops::concatenate_axis(&[&head, &tail], 3);
        }
        Ok(head)
    }
}

fn input(l: i32, d: i32) -> Array {
    let n = (l * d) as usize;
    let v: Vec<f32> = (0..n)
        .map(|i| (((i * 7) % 97) as f32) / 97.0 - 0.5)
        .collect();
    Array::from_slice(&v, &[1, 1, l, d])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // sliding: dims=rotated=256, base 10000, offset 0
    let slide = PropRope {
        dims: 256,
        rotated: 256,
        base: 10000.0,
    }
    .forward(&input(4, 256), 0)?;
    let f = slide.as_dtype(mlx_rs::Dtype::Float32)?;
    eval(std::slice::from_ref(&f))?;
    let s = f.as_slice::<f32>();
    println!("SLIDE p1 {:?} p3 {:?}", &s[256..264], &s[768..776]);

    // full: dims=512, rotated=128, base 1e6
    let full = PropRope {
        dims: 512,
        rotated: 128,
        base: 1e6,
    }
    .forward(&input(4, 512), 0)?;
    let f = full.as_dtype(mlx_rs::Dtype::Float32)?;
    eval(std::slice::from_ref(&f))?;
    let s = f.as_slice::<f32>();
    println!(
        "FULL p1 {:?} p1tail {:?} p3 {:?}",
        &s[512..520],
        &s[640..648],
        &s[1536..1544]
    );
    Ok(())
}
