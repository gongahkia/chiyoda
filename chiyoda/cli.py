from __future__ import annotations

from pathlib import Path

import click

from chiyoda.analysis.reports import export_figures
from chiyoda.analysis.trajectory_reference import (
    compare_trajectory_reference,
    load_trajectory_table,
)
from chiyoda.studies import StudyBundle, compare_studies, load_study_config, run_study


@click.group()
def cli():
    """Chiyoda v3 — ITED crowd dynamics simulation and research toolkit."""
    pass


def _normalized_values(values: tuple[str, ...], fallback: tuple[str, ...]) -> tuple[str, ...]:
    if values:
        return tuple(dict.fromkeys(value.lower() for value in values))
    return fallback


def _bundle_export_settings(
    bundle: StudyBundle,
    figure_formats: tuple[str, ...] = (),
    table_formats: tuple[str, ...] = (),
    profile: str | None = None,
) -> tuple[tuple[str, ...], tuple[str, ...], str, bool]:
    export_config = bundle.metadata.get("export_config", {})
    default_figures = tuple(export_config.get("formats", ("png", "svg", "pdf")))
    default_tables = tuple(export_config.get("table_formats", ("parquet", "csv")))
    default_profile = str(export_config.get("profile", "paper"))
    include_figures = bool(export_config.get("include_figures", True))

    resolved_figures = _normalized_values(figure_formats, default_figures)
    resolved_tables = _normalized_values(table_formats, default_tables)
    resolved_profile = profile or default_profile
    should_export_figures = include_figures or bool(figure_formats)

    return resolved_figures, resolved_tables, resolved_profile, should_export_figures


def _export_bundle(
    bundle: StudyBundle,
    out_dir: str,
    figure_formats: tuple[str, ...] = (),
    table_formats: tuple[str, ...] = (),
    profile: str | None = None,
) -> None:
    output_dir = Path(out_dir)
    resolved_figures, resolved_tables, resolved_profile, should_export_figures = (
        _bundle_export_settings(bundle, figure_formats, table_formats, profile)
    )
    bundle.export(output_dir, table_formats=resolved_tables)
    if should_export_figures and resolved_figures:
        export_figures(
            bundle,
            output_dir=output_dir / "figures",
            profile=resolved_profile,
            formats=resolved_figures,
        )


@cli.command()
@click.argument("scenario_file")
@click.option("-o", "--out", "out_dir", required=True, help="Output study directory")
@click.option(
    "--format",
    "figure_formats",
    multiple=True,
    default=(),
    help="Figure format(s) to export",
)
@click.option(
    "--table-format",
    "table_formats",
    multiple=True,
    default=(),
    help="Table format(s) to export",
)
@click.option("--profile", default=None, help="Export profile")
def run(scenario_file, out_dir, figure_formats, table_formats, profile):
    """Run a single scenario and export a structured study bundle."""
    bundle = run_study(scenario_file)
    _export_bundle(bundle, out_dir, tuple(figure_formats), tuple(table_formats), profile)
    click.echo(f"Exported study bundle to {out_dir}")


@cli.command()
@click.argument("study_file")
@click.option("-o", "--out", "out_dir", required=True, help="Output study directory")
@click.option(
    "--format",
    "figure_formats",
    multiple=True,
    default=(),
    help="Figure format(s) to export",
)
@click.option(
    "--table-format",
    "table_formats",
    multiple=True,
    default=(),
    help="Table format(s) to export",
)
@click.option("--profile", default=None, help="Export profile")
def sweep(study_file, out_dir, figure_formats, table_formats, profile):
    """Run a study definition with repeated seeds, variants, and sweeps."""
    config = load_study_config(study_file)
    bundle = run_study(config)
    _export_bundle(bundle, out_dir, tuple(figure_formats), tuple(table_formats), profile)
    click.echo(f"Exported sweep study to {out_dir}")


@cli.command()
@click.argument("baseline")
@click.argument("variant")
@click.option("-o", "--out", "out_dir", required=True, help="Output comparison directory")
@click.option(
    "--format",
    "figure_formats",
    multiple=True,
    default=(),
    help="Figure format(s) to export",
)
@click.option(
    "--table-format",
    "table_formats",
    multiple=True,
    default=(),
    help="Table format(s) to export",
)
@click.option("--profile", default=None, help="Export profile")
def compare(baseline, variant, out_dir, figure_formats, table_formats, profile):
    """Compare two exported studies and emit comparison tables and figures."""
    baseline_bundle = StudyBundle.load(baseline)
    result = compare_studies(baseline_bundle, variant)
    output_dir = Path(out_dir)
    resolved_figures, resolved_tables, resolved_profile, should_export_figures = (
        _bundle_export_settings(
            baseline_bundle,
            tuple(figure_formats),
            tuple(table_formats),
            profile,
        )
    )
    result.export(output_dir, table_formats=resolved_tables)
    if should_export_figures and resolved_figures:
        export_figures(
            result,
            output_dir=output_dir / "figures",
            profile=resolved_profile,
            formats=resolved_figures,
        )
    click.echo(f"Exported study comparison to {out_dir}")


@cli.command("export-figures")
@click.argument("study_dir")
@click.option(
    "--format",
    "figure_formats",
    multiple=True,
    default=(),
    help="Figure format(s) to export",
)
@click.option("--profile", default=None, help="Export profile")
def export_figures_command(study_dir, figure_formats, profile):
    """Re-export research figures from an existing study directory."""
    bundle = StudyBundle.load(study_dir)
    export_dir = Path(study_dir) / "figures"
    resolved_figures, _, resolved_profile, should_export_figures = _bundle_export_settings(
        bundle,
        tuple(figure_formats),
        profile=profile,
    )
    if should_export_figures and resolved_figures:
        export_figures(
            bundle,
            output_dir=export_dir,
            profile=resolved_profile,
            formats=resolved_figures,
        )
    click.echo(f"Exported figures to {export_dir}")


@cli.command("compare-trajectory-reference")
@click.argument("simulated")
@click.argument("reference")
@click.option("-o", "--out", "out_file", required=True, help="Output CSV file")
@click.option(
    "--group-by",
    "group_columns",
    multiple=True,
    default=("variant_name",),
    help="Simulated table column(s) used for grouped comparisons",
)
def compare_trajectory_reference_command(simulated, reference, out_file, group_columns):
    """Compare exported Chiyoda trajectories with a reference trajectory table."""
    simulated_frame = load_trajectory_table(simulated)
    reference_frame = load_trajectory_table(reference)
    comparison = compare_trajectory_reference(
        simulated_frame,
        reference_frame,
        group_columns=tuple(group_columns),
    )
    output = Path(out_file)
    output.parent.mkdir(parents=True, exist_ok=True)
    comparison.to_csv(output, index=False)
    click.echo(f"Exported trajectory reference comparison to {output}")


@cli.command()
@click.argument("layout_output")
@click.option("--width", default=30, type=int)
@click.option("--height", default=20, type=int)
def generate(layout_output, width, height):
    """Generate a random text layout and save to file."""
    try:
        from src.generate import generate_layout, save_layout  # type: ignore

        layout = generate_layout(width, height)
        save_layout(layout, layout_output)
        click.echo(f"Generated layout saved to {layout_output}")
    except Exception:
        lines = ["X" * width]
        for _ in range(height - 2):
            lines.append("X" + "." * (width - 2) + "X")
        lines.append("X" * width)
        lines[-1] = lines[-1][: width // 2] + "E" + lines[-1][width // 2 + 1 :]
        with open(layout_output, "w") as handle:
            handle.write("\n".join(lines) + "\n")
        click.echo(f"Generated basic layout saved to {layout_output}")


if __name__ == "__main__":
    cli()
