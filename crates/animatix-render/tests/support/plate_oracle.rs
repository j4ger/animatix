//! Pixel-only oracle; also runnable with `rustc --test` without a GPU/dependencies.
pub const WIDTH: usize = 384;
pub const HEIGHT: usize = 192;
const CELL: usize = 96;
const BLACK: [u8; 4] = [0, 0, 0, 255];

pub struct Cell {
    pub name: &'static str,
    color: [u8; 4],
    bounds: [usize; 4], // inclusive, cell-local, allowing 2px AA/stroke tolerance
    area: [usize; 2],
    inside: &'static [(usize, usize)],
    outside: &'static [(usize, usize)],
}

pub const CELLS: [Cell; 8] = [
    Cell {
        name: "rect",
        color: [255, 0, 0, 255],
        bounds: [20, 28, 75, 67],
        area: [2150, 2350],
        inside: &[(48, 48), (23, 31)],
        outside: &[(16, 48)],
    },
    Cell {
        name: "rounded",
        color: [0, 255, 0, 255],
        bounds: [20, 28, 75, 67],
        area: [2000, 2200],
        inside: &[(48, 48), (48, 30)],
        outside: &[(21, 29)],
    },
    Cell {
        name: "ellipse",
        color: [0, 0, 255, 255],
        bounds: [20, 28, 75, 67],
        area: [1650, 1850],
        inside: &[(48, 48), (48, 32)],
        outside: &[(22, 30)],
    },
    Cell {
        name: "polygon",
        color: [255, 255, 0, 255],
        bounds: [20, 28, 75, 67],
        area: [1050, 1250],
        inside: &[(48, 48), (48, 34), (26, 48)],
        outside: &[(24, 32), (28, 64)],
    },
    Cell {
        name: "closed_line_path",
        color: [255, 0, 255, 255],
        bounds: [20, 28, 75, 67],
        area: [1050, 1250],
        inside: &[(48, 48), (48, 62), (28, 64)],
        outside: &[(24, 32), (26, 48)],
    },
    Cell {
        name: "line",
        color: [0, 255, 255, 255],
        bounds: [20, 44, 75, 51],
        area: [420, 480],
        inside: &[(48, 48), (24, 48)],
        outside: &[(48, 40)],
    },
    Cell {
        name: "arrow",
        color: [255, 255, 255, 255],
        bounds: [20, 38, 79, 57],
        area: [330, 500],
        inside: &[(30, 48), (59, 52)],
        outside: &[(30, 40)],
    },
    Cell {
        name: "outline",
        color: [255, 0, 0, 255],
        bounds: [18, 26, 77, 69],
        area: [700, 850],
        inside: &[(20, 48), (48, 28)],
        outside: &[(48, 48)],
    },
];

fn origin(index: usize) -> (usize, usize) {
    ((index % 4) * CELL, (index / 4) * CELL)
}

fn pixel(rgba: &[u8], index: usize, x: usize, y: usize) -> [u8; 4] {
    let (ox, oy) = origin(index);
    let offset = ((oy + y) * WIDTH + ox + x) * 4;
    rgba[offset..offset + 4].try_into().unwrap()
}

fn near(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 8)
}

pub fn failures(rgba: &[u8]) -> Vec<String> {
    assert_eq!(rgba.len(), WIDTH * HEIGHT * 4, "plate dimensions/stride");
    CELLS
        .iter()
        .enumerate()
        .flat_map(|(i, cell)| check_cell(rgba, i, cell))
        .collect()
}

fn check_cell(rgba: &[u8], index: usize, cell: &Cell) -> Vec<String> {
    let mut errors = Vec::new();
    let mut fail = |message: String| errors.push(format!("{}: {message}", cell.name));
    for &(x, y) in cell.inside {
        let actual = pixel(rgba, index, x, y);
        if !near(actual, cell.color) {
            fail(format!("interior ({x},{y}) expected {:?}, got {actual:?}", cell.color));
        }
    }
    for &(x, y) in cell.outside {
        if !near(pixel(rgba, index, x, y), BLACK) {
            fail(format!("exterior ({x},{y}) is not black"));
        }
    }
    let mut bounds = [CELL, CELL, 0, 0];
    let mut area = 0;
    for y in 0..CELL {
        for x in 0..CELL {
            let px = pixel(rgba, index, x, y);
            if (x < 8 || y < 8 || x >= CELL - 8 || y >= CELL - 8) && !near(px, BLACK) {
                fail(format!("gutter contaminated at ({x},{y})"));
                return errors;
            }
            if px[..3].iter().any(|&channel| channel > 32) {
                area += 1;
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
            }
        }
    }
    if area < cell.area[0] || area > cell.area[1] {
        fail(format!("coverage {area}, expected {:?}", cell.area));
    }
    if bounds.iter().zip(cell.bounds).any(|(a, b)| a.abs_diff(b) > 2) {
        fail(format!("bounds {bounds:?}, expected {:?} ±2px", cell.bounds));
    }
    errors
}

pub fn mutate(rgba: &[u8], index: usize, fault: &str) -> Vec<u8> {
    let mut result = rgba.to_vec();
    let (ox, oy) = origin(index);
    for y in 0..CELL {
        for x in 0..CELL {
            let before = pixel(rgba, index, x, y);
            let after = match fault {
                "shape-swap" => {
                    assert!(index == 3 || index == 4, "swap only polygon/path");
                    let source = pixel(rgba, 7 - index, x, y);
                    let coverage = *source[..3].iter().max().unwrap();
                    let mut recolored = CELLS[index].color;
                    for channel in &mut recolored[..3] {
                        *channel = ((*channel as u16 * coverage as u16) / 255) as u8;
                    }
                    recolored
                },
                "disappear" => BLACK,
                "wrong-color" if before[..3].iter().any(|&v| v > 32) => [96, 96, 96, 255],
                "wrong-color" => before,
                "offset" if x >= 8 => pixel(rgba, index, x - 8, y),
                "offset" => BLACK,
                _ => panic!("unknown fault {fault}"),
            };
            let offset = ((oy + y) * WIDTH + ox + x) * 4;
            result[offset..offset + 4].copy_from_slice(&after);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_rejects_local_faults_without_gpu() {
        // A deliberately simple CPU fixture tests the oracle, not the renderer.
        let mut rgba = BLACK.repeat(WIDTH * HEIGHT);
        for y in 28..68 {
            for x in 20..76 {
                let offset = (y * WIDTH + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&CELLS[0].color);
            }
        }
        assert!(check_cell(&rgba, 0, &CELLS[0]).is_empty());
        for fault in ["disappear", "wrong-color", "offset"] {
            let damaged = mutate(&rgba, 0, fault);
            assert!(!check_cell(&damaged, 0, &CELLS[0]).is_empty(), "missed {fault}");
            // Everything outside the changed cell remains byte-identical.
            for index in 1..CELLS.len() {
                for y in 0..CELL {
                    for x in 0..CELL {
                        assert_eq!(pixel(&rgba, index, x, y), pixel(&damaged, index, x, y));
                    }
                }
            }
        }
    }
}
