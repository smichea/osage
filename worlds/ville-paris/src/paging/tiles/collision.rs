// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Layer, Shape};
use anyhow::{Context, Result, bail, ensure};

const BOX_VERTICES: [usize; 24] = [0, 1, 2, 3, 4, 5, 6, 7, 3, 2, 5, 4, 2, 1, 6, 5, 1, 0, 7, 6, 0, 3, 4, 7];
type Mesh = (Vec<[f32; 3]>, Vec<[u32; 3]>);

impl Layer {
    pub(in crate::paging) fn collision_extension(&self) -> &'static str {
        if self.content.file.ends_with(".cpage") { "cpage" } else { "json" }
    }

    pub(in crate::paging) fn decode_collision(&self, data: &[u8]) -> Result<Vec<Shape>> {
        if self.collision_extension() == "json" {
            return Ok(serde_json::from_slice(data)?);
        }
        let mut input = Input(data);
        ensure!(input.take(4)? == b"OCP1", "invalid collision page signature");
        let count = usize::try_from(input.u32()?)?;
        // A one-vertex generic record with one triangle occupies at least 24 bytes.
        ensure!(
            count != 0
                && count == self.ids.len()
                && count <= input.0.len() / 24
                && (self.source_nodes.is_empty() || self.source_nodes.len() == count)
                && (self.materials.is_empty()
                    || self.materials.len() == count && self.materials.iter().all(|m| m.valid())),
            "collision page shape directory mismatch"
        );
        let mut shapes = Vec::new();
        shapes.try_reserve_exact(count)?;
        let mut remaining = self.triangles;
        for slot in 0..count {
            let tag = input.take(1)?[0];
            let (vertices, triangles) = match tag {
                0 => generic(&mut input, remaining)?,
                1..=3 | 0x81..=0x83 => {
                    ensure!(remaining >= 12, "collision box exceeds the directory triangle count");
                    box_mesh(&mut input, tag)?
                }
                _ => bail!("invalid collision shape tag {tag}"),
            };
            ensure!(
                vertices.iter().all(|p| (0..3).all(|i| p[i] >= self.bounds[0][i] && p[i] <= self.bounds[1][i])),
                "collision vertex outside page bounds"
            );
            remaining -= triangles.len();
            shapes.push(Shape {
                faces: Default::default(),
                material: None,
                id: self.ids[slot].clone(),
                source_node: self.source_nodes.get(slot).copied(),
                vertices,
                triangles,
            });
        }
        ensure!(remaining == 0 && input.0.is_empty(), "collision page length or triangle count mismatch");
        Ok(shapes)
    }
}

struct Input<'a>(&'a [u8]);

impl<'a> Input<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let (bytes, rest) = self.0.split_at_checked(n).context("truncated collision page")?;
        self.0 = rest;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn floats<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut result = [0.; N];
        for value in &mut result {
            *value = f32::from_bits(self.u32()?);
            ensure!(value.is_finite(), "non-finite collision coordinate");
        }
        Ok(result)
    }
}

fn generic(input: &mut Input, remaining: usize) -> Result<Mesh> {
    let nv = usize::try_from(input.u32()?)?;
    let nt = usize::try_from(input.u32()?)?;
    ensure!(nv > 0 && nt > 0 && nt <= remaining, "invalid generic collision counts");
    let width = if nv <= 256 {
        1
    } else if nv <= 65_536 {
        2
    } else {
        4
    };
    let mut coordinates = Input(input.take(nv.checked_mul(12).context("collision vertex length overflow")?)?);
    let indices = input.take(nt.checked_mul(3 * width).context("collision triangle length overflow")?)?;
    // Both complete buffers are present before either claimed count can allocate memory.
    let mut vertices = Vec::new();
    vertices.try_reserve_exact(nv)?;
    for _ in 0..nv {
        vertices.push(coordinates.floats()?);
    }
    let mut triangles = Vec::new();
    triangles.try_reserve_exact(nt)?;
    for face in indices.chunks_exact(3 * width) {
        let mut triangle = [0; 3];
        for (dst, index) in triangle.iter_mut().zip(face.chunks_exact(width)) {
            *dst = match width {
                1 => u32::from(index[0]),
                2 => u32::from(u16::from_le_bytes(index.try_into()?)),
                _ => u32::from_le_bytes(index.try_into()?),
            };
            ensure!(usize::try_from(*dst)? < nv, "collision index outside vertex buffer");
        }
        triangles.push(triangle);
    }
    Ok((vertices, triangles))
}

fn box_mesh(input: &mut Input, tag: u8) -> Result<Mesh> {
    let corners = match tag & 0x7f {
        1 => {
            let mut corners = [[0.; 3]; 8];
            for point in &mut corners {
                *point = input.floats()?;
            }
            corners
        }
        form => {
            let [a, b, y, c, d, e, f, top, g, h] = if form == 2 {
                input.floats()?
            } else {
                let [a, b, y, c, d, top] = input.floats()?;
                [a, b, y, c, d, b, a, top, c, d]
            };
            [[a, y, c], [a, y, d], [b, y, d], [b, y, c], [e, top, g], [e, top, h], [f, top, h], [f, top, g]]
        }
    };
    let vertices = BOX_VERTICES.iter().map(|&corner| corners[corner]).collect();
    let mut triangles = Vec::with_capacity(12);
    for face in 0..6 {
        let k = 4 * face;
        for mut t in [[k, k + 1, k + 2], [k, k + 2, k + 3]] {
            if tag & 0x80 != 0 {
                t.swap(1, 2);
            }
            triangles.push(t);
        }
    }
    Ok((vertices, triangles))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paging::tiles::File;

    fn layer(triangles: usize) -> Layer {
        Layer {
            content: File { file: "test.cpage".into(), ..File::default() },
            bounds: [[-100.; 3], [100.; 3]],
            triangles,
            ids: vec!["road".into()],
            source_nodes: vec![42],
            ..Layer::default()
        }
    }

    fn page(tag: u8) -> Vec<u8> {
        let mut bytes = b"OCP1".to_vec();
        bytes.extend(1u32.to_le_bytes());
        bytes.push(tag);
        bytes
    }

    fn append_floats(bytes: &mut Vec<u8>, values: impl IntoIterator<Item = f32>) {
        bytes.extend(values.into_iter().flat_map(f32::to_le_bytes));
    }

    #[test]
    fn all_box_forms_preserve_the_order_winding_and_signed_zero_of_the_source() {
        let regular = [
            [-0., 3., -4.],
            [-0., 3., 5.],
            [2., 3., 5.],
            [2., 3., -4.],
            [2., 7., -4.],
            [2., 7., 5.],
            [-0., 7., 5.],
            [-0., 7., -4.],
        ];
        for reversed in [0, 0x80] {
            for base in [1, 2, 3] {
                let mut corners = regular;
                let mut packed = page(base | reversed);
                match base {
                    1 => {
                        corners[2] = [-9., 6., -11.];
                        corners[6] = [12., -8., 23.];
                        append_floats(&mut packed, corners.into_iter().flatten());
                    }
                    2 => {
                        corners[4..].copy_from_slice(&[[8., 7., 9.], [8., 7., -1.], [-5., 7., -1.], [-5., 7., 9.]]);
                        append_floats(&mut packed, [-0., 2., 3., -4., 5., 8., -5., 7., 9., -1.]);
                    }
                    _ => append_floats(&mut packed, [-0., 2., 3., -4., 5., 7.]),
                }
                let expected = BOX_VERTICES.map(|i| corners[i].map(f32::to_bits));
                let mut directory = layer(12);
                let mut entry = directory.decode_collision(&packed).unwrap().pop().unwrap();
                assert_eq!((entry.id.as_str(), entry.source_node), ("road", Some(42)));
                assert_eq!(entry.vertices.iter().map(|p| p.map(f32::to_bits)).collect::<Vec<_>>(), expected);
                if reversed != 0 {
                    for t in &mut entry.triangles {
                        t.swap(1, 2);
                    }
                }
                assert_eq!(entry.triangles[..2], [[0, 1, 2], [0, 2, 3]]);
                assert_eq!(entry.triangles[10..], [[20, 21, 22], [20, 22, 23]]);
                directory.source_nodes.clear();
                assert_eq!(directory.decode_collision(&packed).unwrap()[0].source_node, None);
            }
        }
    }

    fn generic_page(n: u32) -> Vec<u8> {
        let mut bytes = page(0);
        bytes.extend(n.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        for i in 0..n {
            append_floats(&mut bytes, [i.rem_euclid(7) as f32, -0., 1.]);
        }
        for index in [0, n - 1, 1] {
            let width = if n <= 256 {
                1
            } else if n <= 65_536 {
                2
            } else {
                4
            };
            bytes.extend(&index.to_le_bytes()[..width]);
        }
        bytes
    }

    #[test]
    fn generic_triangles_use_the_smallest_width_for_the_highest_valid_index() {
        for n in [3, 256, 257, 65_536, 65_537] {
            let source = generic_page(n);
            let shape = layer(1).decode_collision(&source).unwrap().pop().unwrap();
            assert_eq!(shape.vertices.len(), n as usize);
            assert_eq!(shape.vertices[0][1].to_bits(), (-0.0f32).to_bits());
            assert_eq!(shape.triangles, [[0, n - 1, 1]]);
        }
    }

    #[test]
    fn a_malformed_record_fails_before_its_geometry_is_published() {
        let original = generic_page(3);
        let check = |change: fn(&mut Vec<u8>)| {
            let mut bytes = original.clone();
            change(&mut bytes);
            assert!(layer(1).decode_collision(&bytes).is_err());
        };
        check(|b| b[2] = b'X');
        check(|b| b[4..8].copy_from_slice(&2u32.to_le_bytes()));
        check(|b| b[8] = 0x80);
        check(|b| b[8] = 4);
        check(|b| b[9..13].copy_from_slice(&u32::MAX.to_le_bytes()));
        check(|b| b[13..17].copy_from_slice(&0u32.to_le_bytes()));
        check(|b| b[17..21].copy_from_slice(&f32::NAN.to_le_bytes()));
        check(|b| b[17..21].copy_from_slice(&101f32.to_le_bytes()));
        check(|b| *b.last_mut().unwrap() = 3);
        check(|b| {
            b.pop();
        });
        check(|b| b.push(0));
        assert!(layer(2).decode_collision(&original).is_err());
        let mut short = page(1);
        short.extend([0; 95]);
        assert!(layer(12).decode_collision(&short).is_err());
    }
}
