// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Input, RESULTS, append};
use anyhow::{Context, Result, bail, ensure};

fn unsigned(input: &mut Input<'_>) -> Result<i64> {
    let mut value = 0i64;
    for shift in (0..63).step_by(7) {
        let byte = input.byte()?;
        value |= i64::from(byte & 127) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    bail!("invalid coarse variable integer")
}

fn signed(input: &mut Input<'_>) -> Result<i64> {
    let value = unsigned(input)?;
    Ok((value >> 1) ^ -(value & 1))
}

pub(super) fn numbers(data: &[u8], limit: usize) -> Result<Vec<i64>> {
    let mut input = Input(data);
    let mode = input.byte()?;
    let width = if mode == 4 { input.byte()? as usize } else { 0 };
    let count = input.count(limit.min(RESULTS))?;
    let mut result = Vec::new();
    result.try_reserve_exact(count)?;
    match mode {
        0 => ensure!(count == 0, "invalid empty coarse integer stream"),
        1 => result.resize(count, signed(&mut input)?),
        2 => {
            let first = signed(&mut input)?;
            let stride = signed(&mut input)?;
            for i in 0..count {
                let x = i128::from(first) + i as i128 * i128::from(stride);
                result.push(i64::try_from(x)?);
            }
        }
        3 => {
            let mut previous = 0i64;
            for _ in 0..count {
                previous = previous.checked_add(signed(&mut input)?).context("coarse integer overflow")?;
                result.push(previous);
            }
        }
        4 => {
            ensure!((1..=8).contains(&width), "invalid coarse integer width");
            let data = input.take(count.checked_mul(width).context("coarse plane size overflow")?)?;
            for i in 0..count {
                let mut word = [0u8; 8];
                for j in 0..width {
                    word[j] = data[j * count + i];
                }
                result.push(i64::try_from(u64::from_le_bytes(word))?);
            }
        }
        5 => {
            for _ in 0..count {
                result.push(unsigned(&mut input)?);
            }
        }
        _ => bail!("unknown coarse integer mode {mode}"),
    }
    input.end()?;
    Ok(result)
}

pub(super) fn indices(data: &[u8], limit: usize) -> Result<Vec<usize>> {
    numbers(data, limit)?.into_iter().map(|n| usize::try_from(n).context("negative coarse index")).collect()
}

enum Column<'a> {
    Constant(&'a [u8]),
    Integers(Vec<i64>),
    Values(Vec<&'a [u8]>),
}

impl<'a> Column<'a> {
    fn read(data: &'a [u8], count: usize) -> Result<Self> {
        let mut input = Input(data);
        Ok(match input.byte()? {
            0 => Self::Constant(input.0),
            1 => {
                let values = numbers(input.0, count)?;
                ensure!(values.len() == count, "invalid coarse numeric column length");
                Self::Integers(values)
            }
            2 => {
                let values = input.0.split(|&c| c == 0).take(count + 1).collect::<Vec<_>>();
                ensure!(values.len() == count, "invalid coarse text column length");
                Self::Values(values)
            }
            3 => {
                let table = input.blob()?.split(|&c| c == 0).take(count + 1).collect::<Vec<_>>();
                ensure!(table.len() <= count, "invalid coarse text dictionary");
                let keys = indices(input.0, count)?;
                ensure!(keys.len() == count, "invalid coarse text reference count");
                let values = keys
                    .into_iter()
                    .map(|i| table.get(i).copied().context("invalid coarse text reference"))
                    .collect::<Result<_>>()?;
                Self::Values(values)
            }
            mode => bail!("unknown coarse text mode {mode}"),
        })
    }

    fn append(&self, row: usize, out: &mut Vec<u8>, limit: usize) -> Result<()> {
        match self {
            Self::Constant(s) => append(out, s, limit),
            Self::Integers(values) => {
                append(out, values.get(row).context("missing numerical field")?.to_string().as_bytes(), limit)
            }
            Self::Values(values) => append(out, values.get(row).context("missing textual field")?, limit),
        }
    }
}

struct Template<'a> {
    first: &'a [u8],
    columns: Vec<(Column<'a>, &'a [u8])>,
    next: usize,
}

fn rows(data: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<()> {
    let mut input = Input(data);
    let order = indices(input.blob()?, limit)?;
    let mut counts = vec![0; input.count(order.len())?];
    for &idx in &order {
        *counts.get_mut(idx).context("invalid coarse row template")? += 1;
    }
    let mut templates = Vec::new();
    let mut numeric_budget = limit;
    for count in counts {
        ensure!(count > 0, "unused coarse row template");
        let mut pieces = input.blob()?.split(|&c| c == 0);
        let first = pieces.next().unwrap_or_default();
        let mut columns = Vec::new();
        for part in pieces {
            numeric_budget = numeric_budget.checked_sub(count).context("coarse row expansion exceeds bound")?;
            columns.push((Column::read(input.blob()?, count)?, part));
        }
        templates.push(Template { first, columns, next: 0 });
    }
    input.end()?;
    append(out, b"[", limit)?;
    for (n, idx) in order.into_iter().enumerate() {
        if n > 0 {
            append(out, b",", limit)?;
        }
        let row = &mut templates[idx];
        append(out, row.first, limit)?;
        for (column, suffix) in &row.columns {
            column.append(row.next, out, limit)?;
            append(out, suffix, limit)?;
        }
        row.next += 1;
    }
    append(out, b"]", limit)
}

pub(super) fn metadata(data: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<()> {
    let mut input = Input(data);
    let fields = input.count(limit)?;
    append(out, b"{", limit)?;
    for i in 0..fields {
        if i > 0 {
            append(out, b",", limit)?;
        }
        append(out, input.blob()?, limit)?;
        append(out, b":", limit)?;
        let mut field = Input(input.blob()?);
        match field.byte()? {
            0 => append(out, field.0, limit)?,
            1 => rows(field.0, out, limit)?,
            2 => {
                let count = field.count(limit.min(RESULTS))?;
                field.end()?;
                append(out, br#"[{"nodes":["#, limit)?;
                for j in 0..count {
                    if j != 0 {
                        append(out, b",", limit)?;
                    }
                    append(out, j.to_string().as_bytes(), limit)?;
                }
                append(out, b"]}]", limit)?;
            }
            mode => bail!("invalid coarse metadata mode {mode}"),
        }
    }
    input.end()?;
    append(out, b"}", limit)
}
