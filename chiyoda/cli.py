from __future__ import annotations

import json
from pathlib import Path

import click

from chiyoda.analysis.info_safety_frontier import check_info_safety_scenario
from chiyoda.analysis.metrics import SimulationAnalytics
from chiyoda.analysis.reports import export_figures, export_policy_brief
from chiyoda.analysis.trajectory_reference import (
    compare_trajectory_reference,
    load_trajectory_table,
)
from chiyoda.analysis.viewer import export_viewer
from chiyoda.environment.gtfs_pathways import strict_scenario_from_gtfs_pathways
from chiyoda.information.route_choice_calibration import (
    fit_route_choice_priors,
    load_figshare_route_choice_records,
    write_normalized_records,
    write_route_choice_fit,
)
from chiyoda.information.warfare import AttackerObjective
from chiyoda.policies import (
    DEFAULT_PPO_BASELINE,
    evaluate_baseline,
    oracle_policy_for_scenario,
    policy_from_artifact,
    train_ppo_baseline,
)
from chiyoda.scenarios.assertions import evaluate_scenario_assertions
from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.scenarios import strict_scenario_from_ifc
from chiyoda.scenarios.standards import (
    OVERPASS_URL,
    strict_scenario_from_geojson,
    strict_scenario_from_osm_bbox,
)
from chiyoda.scenarios.validation import validate_scenario_file
from chiyoda.studies import (
    StudyBundle,
    compare_bundles,
    compare_studies,
    load_study_config,
    run_counterfactual_pair,
    run_study,
    submit_policy,
    validate_submission_file,
)


@click.group()
def cli():
    """Chiyoda v3 — ITED crowd dynamics simulation and research toolkit."""
    pass


def _normalized_values(
    values: tuple[str, ...], fallback: tuple[str, ...]
) -> tuple[str, ...]:
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
    default_figures = tuple(export_config.get("formats", ("png", "svg")))
    default_tables = tuple(export_config.get("table_formats", ("parquet", "csv")))
    default_profile = str(export_config.get("profile", "report"))
    include_figures = bool(export_config.get("include_figures", True))

    resolved_figures = _normalized_values(figure_formats, default_figures)
    resolved_tables = _normalized_values(table_formats, default_tables)
    resolved_profile = profile or default_profile
    should_export_figures = include_figures or bool(figure_formats)

    return resolved_figures, resolved_tables, resolved_profile, should_export_figures


def _dump_debug_steps(bundle: StudyBundle, out_dir: str) -> None:
    """Write per-step telemetry to ``<out_dir>/debug_steps.jsonl``."""
    output_dir = Path(out_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    target = output_dir / "debug_steps.jsonl"
    steps = getattr(bundle, "steps", None)
    if steps is None or getattr(steps, "empty", True):
        target.write_text("")
        return
    with target.open("w") as handle:
        for record in steps.to_dict(orient="records"):
            handle.write(json.dumps(record, default=str) + "\n")


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
    export_viewer(bundle, output_dir=output_dir / "viewer")


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
@click.option(
    "--debug",
    is_flag=True,
    default=False,
    help="Dump per-step telemetry to <out>/debug_steps.jsonl",
)
@click.option(
    "--counterfactual",
    is_flag=True,
    default=False,
    help="Run treated and no-intervention matched bundles plus causal_delta.json",
)
@click.option(
    "--counterfactual-repetitions",
    default=1,
    type=int,
    help="Matched seed repetitions for --counterfactual",
)
@click.option(
    "--counterfactual-bootstrap-samples",
    default=1000,
    type=int,
    help="Bootstrap samples for --counterfactual causal intervals",
)
def run(
    scenario_file,
    out_dir,
    figure_formats,
    table_formats,
    profile,
    debug,
    counterfactual,
    counterfactual_repetitions,
    counterfactual_bootstrap_samples,
):
    """Run a single scenario and export a structured study bundle."""
    if counterfactual:
        result = run_counterfactual_pair(
            scenario_file,
            repetitions=int(counterfactual_repetitions),
            bootstrap_samples=int(counterfactual_bootstrap_samples),
        )
        output_dir = Path(out_dir)
        _export_bundle(
            result["baseline"],
            str(output_dir / "no_intervention"),
            tuple(figure_formats),
            tuple(table_formats),
            profile,
        )
        _export_bundle(
            result["treated"],
            str(output_dir / "treated"),
            tuple(figure_formats),
            tuple(table_formats),
            profile,
        )
        output_dir.mkdir(parents=True, exist_ok=True)
        (output_dir / "causal_delta.json").write_text(
            json.dumps(result["causal_delta"], indent=2, sort_keys=True) + "\n"
        )
        click.echo(f"Exported counterfactual study bundle to {out_dir}")
        return
    bundle = run_study(scenario_file)
    _export_bundle(
        bundle, out_dir, tuple(figure_formats), tuple(table_formats), profile
    )
    if debug:
        _dump_debug_steps(bundle, out_dir)
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
@click.option("--jobs", default=None, type=int, help="Parallel seed jobs")
def sweep(study_file, out_dir, figure_formats, table_formats, profile, jobs):
    """Run a study definition with repeated seeds, variants, and sweeps."""
    config = load_study_config(study_file)
    if jobs is not None:
        config = config.model_copy(update={"jobs": jobs})
    bundle = run_study(config)
    _export_bundle(
        bundle, out_dir, tuple(figure_formats), tuple(table_formats), profile
    )
    click.echo(f"Exported sweep study to {out_dir}")


@cli.command("validate-scenario")
@click.argument("scenario_file")
@click.option(
    "--json",
    "json_output",
    is_flag=True,
    help="Emit machine-readable validation output",
)
@click.pass_context
def validate_scenario_command(ctx, scenario_file, json_output):
    """Validate scenario layout starts, exits, and static exit reachability."""
    result = validate_scenario_file(scenario_file)
    if json_output:
        click.echo(json.dumps(result.to_dict(), indent=2))
    else:
        status = "ERROR" if result.has_errors else "OK"
        click.echo(
            f"{status}: {scenario_file} "
            f"layout={result.layout_width}x{result.layout_height} "
            f"exits={len(result.exits)} starts={len(result.starts)}"
        )
        for issue in result.issues:
            cell = f" cell={issue.cell}" if issue.cell is not None else ""
            source = f" source={issue.source}" if issue.source else ""
            click.echo(
                f"{issue.severity.upper()} {issue.code}:{cell}{source} {issue.message}"
            )
    if result.has_errors:
        ctx.exit(1)


@cli.command("assert-scenario")
@click.argument("scenario_file")
@click.option(
    "--json", "json_output", is_flag=True, help="Emit machine-readable assertion output"
)
@click.pass_context
def assert_scenario_command(ctx, scenario_file, json_output):
    """Run a scenario and evaluate its runtime assertions."""
    manager = ScenarioManager()
    scenario = manager.load_config(scenario_file)
    simulation = manager.build_simulation(scenario)
    simulation.run()
    result = evaluate_scenario_assertions(scenario, simulation)
    if json_output:
        click.echo(json.dumps(result.to_dict(), indent=2))
    else:
        status = "OK" if result.ok else "ERROR"
        click.echo(f"{status}: {scenario_file} assertions={len(result.issues)}")
        for issue in result.issues:
            click.echo(
                f"ERROR {issue.code}: {issue.message} observed={issue.observed} expected={issue.expected}"
            )
    if not result.ok:
        ctx.exit(1)


@cli.command("info-safety-check")
@click.argument("scenario_file")
@click.option("--json", "json_output", is_flag=True, help="Emit JSON verdict")
def info_safety_check_command(scenario_file, json_output):
    """Flag scenarios likely to increase HCI after entropy-reducing messages."""
    verdict = check_info_safety_scenario(scenario_file)
    payload = verdict.to_dict()
    if json_output:
        click.echo(json.dumps(payload, indent=2))
        return
    click.echo(
        f"{payload['verdict'].upper()}: {scenario_file} "
        f"score={payload['score']:.3f} "
        f"entropy={payload['entropy_reduction_potential']:.3f} "
        f"convergence={payload['convergence_pressure']:.3f} "
        f"queue={payload['queue_pressure']:.3f} "
        f"exposure={payload['exposure_pressure']:.3f}"
    )
    for reason in payload["reasons"]:
        click.echo(f"- {reason}")


@cli.command("convert-layout")
@click.argument("source_or_output")
@click.argument("output_file", required=False)
@click.option("--name", default="converted_station", help="Scenario name")
@click.option("--cell-size", default=1.0, type=float, help="Raster cell size")
@click.option("--padding", default=1, type=int, help="Raster padding in cells")
@click.option(
    "--osm-bbox",
    default=None,
    help="Fetch OSM via Overpass using south,west,north,east lat/lon bbox",
)
@click.option("--overpass-url", default=OVERPASS_URL, help="Overpass interpreter URL")
@click.option("--overpass-timeout", default=25, type=int, help="Overpass timeout")
def convert_layout_command(
    source_or_output,
    output_file,
    name,
    cell_size,
    padding,
    osm_bbox,
    overpass_url,
    overpass_timeout,
):
    """Convert OSM/GTFS-like GeoJSON into strict layout.floors/connectors YAML."""
    import yaml

    if osm_bbox:
        payload = strict_scenario_from_osm_bbox(
            osm_bbox,
            name=name,
            cell_size=cell_size,
            padding=padding,
            overpass_url=overpass_url,
            timeout=overpass_timeout,
        )
        output = Path(output_file or source_or_output)
    else:
        if output_file is None:
            raise click.UsageError("convert-layout requires OUTPUT_FILE")
        payload = strict_scenario_from_geojson(
            source_or_output,
            name=name,
            cell_size=cell_size,
            padding=padding,
        )
        output = Path(output_file)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(yaml.safe_dump(payload, sort_keys=False))
    click.echo(f"Exported strict scenario to {output}")


@cli.command("convert-gtfs")
@click.argument("feed_dir")
@click.argument("output_file")
@click.option("--name", default="gtfs_pathways_import", help="Scenario name")
@click.option("--cell-size", default=1.0, type=float, help="Raster cell size")
@click.option("--padding", default=1, type=int, help="Raster padding in cells")
def convert_gtfs_command(feed_dir, output_file, name, cell_size, padding):
    """Convert GTFS Pathways stops/levels/pathways into strict scenario YAML."""
    import yaml

    payload = strict_scenario_from_gtfs_pathways(
        feed_dir,
        name=name,
        cell_size=cell_size,
        padding=padding,
    )
    output = Path(output_file)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(yaml.safe_dump(payload, sort_keys=False))
    click.echo(f"Exported GTFS Pathways scenario to {output}")


@cli.command("convert-ifc")
@click.argument("source")
@click.argument("output_file")
@click.option("--name", default="ifc_import", help="Scenario name")
@click.option("--cell-size", default=1.0, type=float, help="Raster cell size")
@click.option("--padding", default=1, type=int, help="Raster padding in cells")
def convert_ifc_command(source, output_file, name, cell_size, padding):
    """Convert IFC building geometry into strict layout.floors scenario YAML."""
    import yaml

    payload = strict_scenario_from_ifc(
        source,
        name=name,
        cell_size=cell_size,
        padding=padding,
    )
    output = Path(output_file)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(yaml.safe_dump(payload, sort_keys=False))
    click.echo(f"Exported IFC scenario to {output}")


@cli.command("calibrate-route-choice")
@click.option(
    "--archive",
    default="data/calibration/route_choice_2025/Snopkova_Isovists.zip",
    help="Figshare route-choice archive path",
)
@click.option(
    "-o",
    "--out",
    "out_file",
    default="data/calibration/route_choice_2025/fit_parameters.json",
    help="Output fitted parameter JSON",
)
@click.option(
    "--normalized-out",
    default="data/calibration/route_choice_2025/normalized_route_choice_records.csv",
    help="Output normalized observation CSV",
)
def calibrate_route_choice_command(archive, out_file, normalized_out):
    """Fit route-choice priors from the 2025 Figshare evacuation dataset."""
    records = load_figshare_route_choice_records(archive)
    fit = fit_route_choice_priors(records)
    write_normalized_records(records, normalized_out)
    write_route_choice_fit(fit, out_file)
    click.echo(json.dumps(fit.to_dict(), indent=2, sort_keys=True))


@cli.command()
@click.argument("baseline")
@click.argument("variant")
@click.option(
    "-o", "--out", "out_dir", required=True, help="Output comparison directory"
)
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
    export_policy_brief(result, output_dir)
    click.echo(f"Exported study comparison to {out_dir}")


@cli.command("causal-compare")
@click.argument("baseline_bundle")
@click.argument("treated_bundle")
@click.option(
    "--metric",
    "metrics",
    multiple=True,
    required=True,
    help="Summary metric to compare",
)
@click.option(
    "--estimator", default="ate", type=click.Choice(["ate"]), help="Estimator"
)
@click.option(
    "--bootstrap-samples", default=1000, type=int, help="Bootstrap sample count"
)
@click.option("--random-seed", default=42, type=int, help="Bootstrap random seed")
@click.option("-o", "--out", "out_file", default=None, help="Optional CSV output path")
def causal_compare_command(
    baseline_bundle,
    treated_bundle,
    metrics,
    estimator,
    bootstrap_samples,
    random_seed,
    out_file,
):
    """Compare matched-seed counterfactual study bundles."""
    baseline = StudyBundle.load(baseline_bundle)
    treated = StudyBundle.load(treated_bundle)
    result = compare_bundles(
        baseline,
        treated,
        metrics=tuple(metrics),
        estimator=estimator,
        bootstrap_samples=bootstrap_samples,
        random_seed=random_seed,
    )
    if out_file is not None:
        output = Path(out_file)
        output.parent.mkdir(parents=True, exist_ok=True)
        result.to_csv(output, index=False)
    click.echo(result.to_json(orient="records", indent=2))


@cli.group()
def benchmark():
    """Benchmark suite commands."""
    pass


@benchmark.command("submit")
@click.option("--policy", "policy_path", default=None, help="Policy YAML/JSON path")
@click.option("--suite", default="v1", help="Benchmark suite")
@click.option(
    "-o",
    "--out",
    "out_dir",
    default="out/benchmark_submission",
    help="Output directory",
)
def benchmark_submit_command(policy_path, suite, out_dir):
    """Run a benchmark submission and export leaderboard artifacts."""
    result = submit_policy(policy_path=policy_path, suite=suite, output_dir=out_dir)
    click.echo(json.dumps(result["leaderboard"], indent=2, sort_keys=True))


@benchmark.command("validate-submission")
@click.argument("submission_file")
@click.option("--json", "json_output", is_flag=True, help="Emit JSON validation result")
@click.pass_context
def benchmark_validate_submission_command(ctx, submission_file, json_output):
    """Validate a public benchmark submission JSON file."""
    result = validate_submission_file(submission_file)
    if json_output:
        click.echo(json.dumps(result, indent=2, sort_keys=True))
    elif result["ok"]:
        click.echo(f"OK: {submission_file}")
    else:
        click.echo(f"ERROR: {submission_file}")
        for issue in result["issues"]:
            click.echo(f"{issue['path']}: {issue['message']}")
    if not result["ok"]:
        ctx.exit(1)


@cli.group("baseline")
def baseline_group():
    """Train and evaluate shipped baseline policies."""
    pass


@baseline_group.command("train")
@click.option("--kind", type=click.Choice(["ppo"]), default="ppo")
@click.option("--scenario", default="scenarios/benchmark/transit_cbrn.yaml")
@click.option("--timesteps", default=256, type=int)
@click.option("--seed", default=42, type=int)
@click.option("-o", "--out", "out_dir", default="data/baselines")
@click.option(
    "--allow-fallback",
    is_flag=True,
    help="Write deterministic metadata if stable-baselines3 is absent",
)
def baseline_train_command(kind, scenario, timesteps, seed, out_dir, allow_fallback):
    """Train a baseline policy artifact."""
    del kind
    artifact = train_ppo_baseline(
        scenario_file=scenario,
        output_dir=out_dir,
        total_timesteps=timesteps,
        seed=seed,
        require_sb3=not allow_fallback,
    )
    click.echo(json.dumps(artifact, indent=2, sort_keys=True))


@baseline_group.command("eval")
@click.option("--baseline", type=click.Choice(["oracle", "ppo"]), default="oracle")
@click.option("--suite", default="v1")
@click.option("--weights", default=str(DEFAULT_PPO_BASELINE))
@click.option("-o", "--out", "out_dir", default="out/baseline_eval")
def baseline_eval_command(baseline, suite, weights, out_dir):
    """Evaluate a shipped baseline policy on a benchmark suite."""
    if baseline == "oracle":
        selector = oracle_policy_for_scenario
    else:
        policy = policy_from_artifact(weights)
        selector = lambda scenario: policy
    result = evaluate_baseline(
        baseline=baseline,
        suite=suite,
        output_dir=out_dir,
        policy_selector=selector,
    )
    click.echo(json.dumps(result["leaderboard"], indent=2, sort_keys=True))


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
    resolved_figures, _, resolved_profile, should_export_figures = (
        _bundle_export_settings(
            bundle,
            tuple(figure_formats),
            profile=profile,
        )
    )
    if should_export_figures and resolved_figures:
        export_figures(
            bundle,
            output_dir=export_dir,
            profile=resolved_profile,
            formats=resolved_figures,
        )
    click.echo(f"Exported figures to {export_dir}")


@cli.command("export-viewer")
@click.argument("study_dir")
def export_viewer_command(study_dir):
    """Export a static Three.js viewer from an existing study directory."""
    bundle = StudyBundle.load(study_dir)
    export_dir = Path(study_dir) / "viewer"
    export_viewer(bundle, output_dir=export_dir)
    click.echo(f"Exported viewer to {export_dir}")


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


@cli.command("red-team")
@click.argument("scenario_file")
@click.option("--budget", default=10, type=int, help="Maximum hostile messages")
@click.option(
    "--objective",
    default=AttackerObjective.FALSE_PROTECTIVE_ACTION.value,
    type=click.Choice([item.value for item in AttackerObjective]),
    help="Attacker objective",
)
@click.option("--channel-type", default="gossip", help="Hostile channel type")
@click.option("--plausibility", default=0.65, type=float, help="Claim plausibility")
@click.option("--interval-steps", default=1, type=int, help="Injection interval")
@click.option(
    "-o", "--out", "out_file", default=None, help="Optional JSON summary path"
)
def red_team_command(
    scenario_file,
    budget,
    objective,
    channel_type,
    plausibility,
    interval_steps,
    out_file,
):
    """Run a scenario with an injected hostile information channel."""
    manager = ScenarioManager()
    scenario = manager.load_config(scenario_file)
    channels = list(scenario.get("hostile_channels") or [])
    injected = {
        "id": "red_team_cli",
        "channel_type": channel_type,
        "objective": objective,
        "budget": int(budget),
        "plausibility": float(plausibility),
        "interval_steps": max(1, int(interval_steps)),
        "start_step": 0,
    }
    if channels:
        channels[0] = {**channels[0], **injected}
    else:
        channels.append(injected)
    scenario["hostile_channels"] = channels

    simulation = manager.build_simulation(scenario)
    simulation.run()
    metrics = SimulationAnalytics().calculate_performance_metrics(simulation)
    payload = {
        "scenario": scenario.get("name", Path(scenario_file).stem),
        "objective": objective,
        "budget": int(budget),
        "hostile_events": len(getattr(simulation, "hostile_channel_events", [])),
        "metrics": metrics,
    }
    text = json.dumps(payload, indent=2, sort_keys=True)
    if out_file is not None:
        output = Path(out_file)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text + "\n")
    click.echo(text)


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


def main() -> None:
    """Console script entrypoint for the ``chiyoda`` command."""
    cli()


if __name__ == "__main__":
    main()
