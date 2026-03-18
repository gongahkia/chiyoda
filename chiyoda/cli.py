from __future__ import annotations

import click

from chiyoda.scenarios.manager import ScenarioManager
from chiyoda.visualization.plotly_viz import InteractiveVisualizer
from chiyoda.analysis.reports import generate_report


@click.group()
def cli():
    """Chiyoda v2 - Commuter Dynamics Simulator"""
    pass


@cli.command()
@click.argument("scenario_file")
@click.option("-o", "--output", "output", default="simulation.html", help="Export HTML file")
@click.option("--headless", is_flag=True, help="Run without interactive UI and export HTML")
def run(scenario_file, output, headless):
    """Run a simulation from a scenario YAML file."""
    sim = ScenarioManager().load_scenario(scenario_file)
    viz = InteractiveVisualizer()
    if headless:
        sim.run()
        generate_report(sim, output)
        click.echo(f"Exported study dashboard to {output}")
    else:
        viz.init(sim)
        sim.run(visualize=True, visualizer=viz)
        viz.show()


@cli.command()
@click.argument("layout_output")
@click.option("--width", default=30, type=int)
@click.option("--height", default=20, type=int)
def generate(layout_output, width, height):
    """Generate a random text layout and save to file."""
    try:
        # Prefer local generator if available
        from src.generate import generate_layout, save_layout  # type: ignore

        layout = generate_layout(width, height)
        save_layout(layout, layout_output)
        click.echo(f"Generated layout saved to {layout_output}")
    except Exception:
        # Fallback: write a simple empty room with exits
        lines = ["X" * width]
        for _ in range(height - 2):
            lines.append("X" + "." * (width - 2) + "X")
        lines.append("X" * width)
        lines[-1] = lines[-1][: width // 2] + "E" + lines[-1][width // 2 + 1 :]
        with open(layout_output, "w") as f:
            f.write("\n".join(lines) + "\n")
        click.echo(f"Generated basic layout saved to {layout_output}")


if __name__ == "__main__":
    cli()
