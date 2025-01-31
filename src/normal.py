# ----- IMPORTS -----

import matplotlib.pyplot as plt
import numpy as np
from matplotlib.animation import FuncAnimation

# ----- HELPER FUNCTIONS -----


def read_layout(filename):
    """
    read the layout file
    """
    with open(filename, "r") as file:
        layout = [list(line.strip()) for line in file]
    return layout


def setup_simulation(layout):
    """
    setup the simulation
    """
    height = len(layout)
    width = len(layout[0])
    fig, ax = plt.subplots(figsize=(10, 8))
    ax.set_xlim(0, width)
    ax.set_ylim(0, height)
    walls = []
    people = []
    exit_point = None
    for y, row in enumerate(layout):
        for x, cell in enumerate(row):
            if cell == "X":
                walls.append(plt.Rectangle((x, height - y - 1), 1, 1, fc="gray"))
            elif cell == "@":
                people.append([x + 0.5, height - y - 0.5])
            elif cell == "E":
                exit_point = np.array([x + 0.5, height - y - 0.5])
    for wall in walls:
        ax.add_patch(wall)
    if exit_point is None:
        raise ValueError("No exit point (E) found in the layout")
    ax.scatter(exit_point[0], exit_point[1], c="green", s=100, marker="*")
    return fig, ax, np.array(people), exit_point, walls


def update(frame, scatter, people_container, exit_point, walls):
    """
    update the simulation
    """
    people = people_container[0]
    if len(people) == 0:
        return (scatter,)
    direction = exit_point - people
    distance = np.linalg.norm(direction, axis=1).reshape(-1, 1)
    direction /= distance
    noise = np.random.randn(len(people), 2) * 0.1
    new_positions = people + direction + noise
    for wall in walls:
        wall_bounds = wall.get_bbox()
        mask = (
            (new_positions[:, 0] > wall_bounds.x0)
            & (new_positions[:, 0] < wall_bounds.x1)
            & (new_positions[:, 1] > wall_bounds.y0)
            & (new_positions[:, 1] < wall_bounds.y1)
        )
        new_positions[mask] = people[mask]
    new_positions = np.clip(
        new_positions, 0, max(scatter.axes.get_xlim()[1], scatter.axes.get_ylim()[1])
    )
    reached_exit = np.linalg.norm(new_positions - exit_point, axis=1) < 0.5
    people_container[0] = new_positions[~reached_exit]
    scatter.set_offsets(people_container[0])
    return (scatter,)


def run_simulation(layout_file):
    """
    run the simulation with the specified layout file
    """
    layout = read_layout(layout_file)
    fig, ax, people, exit_point, walls = setup_simulation(layout)
    scatter = ax.scatter(people[:, 0], people[:, 1], c="blue", s=10)
    people_container = [people]
    anim = FuncAnimation(
        fig,
        update,
        fargs=(scatter, people_container, exit_point, walls),
        frames=200,
        interval=50,
        blit=True,
    )
    plt.title("Normal movement simulation")
    plt.show()


# ----- SAMPLE EXECUTION CODE -----

if __name__ == "__main__":
    layout_file = "layout.txt"
    run_simulation(layout_file)
