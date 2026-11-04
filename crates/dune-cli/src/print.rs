//! The spice3 `.print` column format, rendered over a
//! [`dune_core::Selection`] — a faithful port of `com_print`'s
//! column mode (`fte/postcoms.c`). Formatting lives only here; every
//! other consumer of results (bindings, oracle) reads the selection
//! as data.
//!
//! Layout, per the C: vectors are split into page-wide blocks
//! (8 columns for `Index`, 16 per real vector, at least two vectors
//! per block), the plot's scale is prepended to every block; each
//! block starts with the centered title, a `plotname  date` line
//! indented by the title's padding (the C computes a fresh centering
//! for it and then never uses it), a dash rule, and the `Index`
//! header; pages break every `height` lines with a form feed.
//! Values are C `%e` (`1.000000e+00`), tab-separated.

use dune_core::{ColData, OutColumn, Selection};
use dune_raw::fmt_e;

/// `DEF_WIDTH`/`DEF_HEIGHT` (`ftedefs.h`), the line-printer page.
pub const DEF_WIDTH: usize = 80;
pub const DEF_HEIGHT: usize = 60;

pub struct PageSetup {
    pub width: usize,
    pub height: usize,
    /// `.options nopage` / `set nobreak`: no pagination at all.
    pub nobreak: bool,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            width: DEF_WIDTH,
            height: DEF_HEIGHT,
            nobreak: false,
        }
    }
}

fn col_width(col: &OutColumn) -> usize {
    match col.data {
        ColData::Real(_) => 16,
        ColData::Complex(_) => 32,
    }
}

/// Render one `.print` card's selection.
pub fn print_columns(
    out: &mut String,
    title: &str,
    plotname: &str,
    date: &str,
    sel: &Selection,
    page: &PageSetup,
) {
    let width = page.width.max(40);
    let height = page.height.max(20);

    // Blocks: prepend the scale to each (com_print's "make the first
    // vector of every page be the scale"), then fit vectors by width.
    let mut remaining: Vec<&OutColumn> = sel.columns.iter().collect();
    let mut first_block = true;
    while !remaining.is_empty() || first_block {
        let mut block: Vec<&OutColumn> = Vec::new();
        if let Some(scale) = &sel.scale {
            if remaining
                .first()
                .map(|c| c.label != scale.label)
                .unwrap_or(true)
            {
                block.push(scale);
            }
        }
        let mut ll = 8;
        while let Some(col) = remaining.first() {
            // "Two tabs for real, 4 for complex."
            ll += col_width(col);
            if ll > width && block.len() >= 2 {
                break;
            }
            block.push(col);
            remaining.remove(0);
        }
        if block.is_empty() {
            break;
        }
        render_block(
            out,
            title,
            plotname,
            date,
            &block,
            width,
            height,
            page.nobreak,
        );
        first_block = false;
        if !remaining.is_empty() {
            out.push_str("\x0c\n"); // form feed between blocks
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_block(
    out: &mut String,
    title: &str,
    plotname: &str,
    date: &str,
    block: &[&OutColumn],
    width: usize,
    height: usize,
    nobreak: bool,
) {
    // Centered title; the date line reuses the title's padding
    // (quirk kept from the C).
    let pad = " ".repeat((width.saturating_sub(title.len())) / 2);
    out.push_str(&pad);
    out.push_str(title);
    out.push('\n');
    out.push_str(&pad);
    out.push_str(&format!("{plotname}  {date}\n"));
    let dashes = "-".repeat(width);
    out.push_str(&dashes);
    out.push('\n');

    // `Index` header: `%-16.15s` per real vector, `%-32.31s` per
    // complex one.
    let mut header = String::from("Index   ");
    for col in block {
        if col_width(col) == 16 {
            header.push_str(&format!("{:<16.15}", col.label));
        } else {
            header.push_str(&format!("{:<32.31}", col.label));
        }
    }

    let npoints = block.iter().map(|c| c.data.len()).max().unwrap_or(0);
    let mut lineno = 3;
    let mut j = 0;
    loop {
        out.push_str(&header);
        out.push('\n');
        out.push_str(&dashes);
        out.push('\n');
        lineno += 2;
        while j < npoints && (nobreak || lineno < height) {
            out.push_str(&format!("{j}\t"));
            for col in block {
                match &col.data {
                    ColData::Real(v) => match v.get(j) {
                        Some(x) => {
                            out.push_str(&fmt_e(*x, 6));
                            out.push('\t');
                        }
                        None => out.push_str("\t\t"),
                    },
                    ColData::Complex(v) => match v.get(j) {
                        Some(z) => {
                            out.push_str(&fmt_e(z.re, 6));
                            out.push_str(",\t");
                            out.push_str(&fmt_e(z.im, 6));
                            out.push('\t');
                        }
                        None => out.push_str("\t\t\t\t"),
                    },
                }
            }
            out.push('\n');
            j += 1;
            lineno += 1;
        }
        if j >= npoints {
            return;
        }
        // New page within the block.
        out.push_str("\x0c\n");
        lineno = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dune_raw::VarType;

    fn sel() -> Selection {
        Selection {
            scale: Some(OutColumn {
                label: "time".into(),
                unit: VarType::Time,
                data: ColData::Real(vec![0.0, 1e-9, 2e-9]),
            }),
            columns: vec![OutColumn {
                label: "v(2)".into(),
                unit: VarType::Voltage,
                data: ColData::Real(vec![0.0, 0.5, -1.25]),
            }],
        }
    }

    #[test]
    fn spice3_column_layout() {
        let mut out = String::new();
        print_columns(
            &mut out,
            "basic rc circuit",
            "Transient Analysis",
            "",
            &sel(),
            &PageSetup::default(),
        );
        let lines: Vec<&str> = out.lines().collect();
        // Centered title: (80-16)/2 = 32 spaces.
        assert_eq!(lines[0], format!("{}basic rc circuit", " ".repeat(32)));
        assert_eq!(lines[1], format!("{}Transient Analysis  ", " ".repeat(32)));
        assert_eq!(lines[2], "-".repeat(80));
        assert_eq!(lines[3], format!("Index   {:<16}{:<16}", "time", "v(2)"));
        assert_eq!(lines[4], "-".repeat(80));
        assert_eq!(lines[5], "0\t0.000000e+00\t0.000000e+00\t");
        assert_eq!(lines[6], "1\t1.000000e-09\t5.000000e-01\t");
        assert_eq!(lines[7], "2\t2.000000e-09\t-1.250000e+00\t");
        assert!(!out.contains('\x0c'));
    }

    #[test]
    fn long_names_truncate_to_15() {
        let mut out = String::new();
        let s = Selection {
            scale: None,
            columns: vec![OutColumn {
                label: "v(a-very-long-node-name)".into(),
                unit: VarType::Voltage,
                data: ColData::Real(vec![1.0]),
            }],
        };
        print_columns(
            &mut out,
            "t",
            "Operating Point",
            "",
            &s,
            &PageSetup::default(),
        );
        let header = out.lines().nth(3).unwrap();
        assert_eq!(
            header,
            format!("Index   {:<16.15}", "v(a-very-long-node-name)")
        );
        assert_eq!(header.len(), 8 + 16);
    }

    #[test]
    fn pagination_emits_form_feeds() {
        let mut out = String::new();
        let s = Selection {
            scale: None,
            columns: vec![OutColumn {
                label: "v(1)".into(),
                unit: VarType::Voltage,
                data: ColData::Real((0..100).map(|i| i as f64).collect()),
            }],
        };
        let page = PageSetup {
            width: 40,
            height: 20,
            nobreak: false,
        };
        print_columns(&mut out, "t", "p", "", &s, &page);
        assert!(out.contains('\x0c'));
        // All 100 rows survive pagination.
        assert_eq!(out.lines().filter(|l| l.ends_with('\t')).count(), 100);
    }

    #[test]
    fn wide_selections_split_into_blocks_with_scale() {
        let mut out = String::new();
        let cols: Vec<OutColumn> = (0..8)
            .map(|k| OutColumn {
                label: format!("v({k})"),
                unit: VarType::Voltage,
                data: ColData::Real(vec![k as f64]),
            })
            .collect();
        let s = Selection {
            scale: Some(OutColumn {
                label: "time".into(),
                unit: VarType::Time,
                data: ColData::Real(vec![0.0]),
            }),
            columns: cols,
        };
        let page = PageSetup {
            width: 40,
            height: 60,
            nobreak: false,
        };
        print_columns(&mut out, "t", "p", "", &s, &page);
        // Every block header carries the scale again.
        let headers: Vec<&str> = out.lines().filter(|l| l.starts_with("Index")).collect();
        assert!(headers.len() > 1);
        assert!(headers.iter().all(|h| h.contains("time")));
    }
}
