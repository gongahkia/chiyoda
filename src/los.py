# ----- IMPORTS -----

import matplotlib.pyplot as plt
import numpy as np
from matplotlib.animation import FuncAnimation
from scipy.ndimage import binary_dilation, generate_binary_structure

# ----- HELPER FUNCTIONS -----


def read_layout(filename):
    """
    read the layout file and ensure consistent row lengths
    """
    with open(filename, "r") as file:
        layout = [list(line.strip()) for line in file]
    max_length = max(len(row) for row in layout)
    layout = [row + [" "] * (max_length - len(row)) for row in layout]
    return layout


def identify_bottlenecks(layout):
    """
    identify potential bottlenecks in the layout
    """
    height = len(layout)
    width = len(layout[0])
    binary_map = np.zeros((height, width), dtype=int)
    for i, row in enumerate(layout):
        for j, cell in enumerate(row):
            if cell == "X":
                binary_map[i, j] = 1
    struct = generate_binary_structure(2, 2)
    dilated = binary_dilation(binary_map, structure=struct, iterations=1)
    narrow_passages = dilated & ~binary_map
    bottlenecks = np.where(narrow_passages)
    return list(
        zip(bottlenecks[1], height - 1 - bottlenecks[0])
    )  # Convert to (x, y) coordinates


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
    bottlenecks = identify_bottlenecks(layout)
    for x, y in bottlenecks:
        circle = plt.Circle((x + 0.5, y + 0.5), 0.5, fill=False, ec="red")
        ax.add_artist(circle)
    return fig, ax, np.array(people), exit_point, walls


def update(frame, scatter, people_container, exit_point, walls, ax):
    """
    update the simulation
    """
    people = people_container[0]
    if len(people) == 0:
        return (scatter,)
    for artist in ax.lines:
        artist.remove()
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
    for i, (person, new_pos) in enumerate(zip(people, new_positions)):
        ax.plot([person[0], exit_point[0]], [person[1], exit_point[1]], "g-", alpha=0.3)
        ax.plot([person[0], new_pos[0]], [person[1], new_pos[1]], "r:", alpha=0.5)
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
        fargs=(scatter, people_container, exit_point, walls, ax),
        frames=200,
        interval=50,
        blit=False,
    )
    plt.title("Line of sight simulation")
    plt.show()


# ----- SAMPLE EXECUTION CODE -----

if __name__ == "__main__":
    layout_file = "layout.txt"
    run_simulation(layout_file)
