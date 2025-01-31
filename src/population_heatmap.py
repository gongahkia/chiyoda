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
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(20, 8))
    ax1.set_xlim(0, width)
    ax1.set_ylim(0, height)
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
        ax1.add_patch(wall)
    if exit_point is None:
        raise ValueError("No exit point (E) found in the layout")
    ax1.scatter(exit_point[0], exit_point[1], c="green", s=100, marker="*")
    bottlenecks = identify_bottlenecks(layout)
    for x, y in bottlenecks:
        circle = plt.Circle((x + 0.5, y + 0.5), 0.5, fill=False, ec="red")
        ax1.add_artist(circle)
    heatmap = np.zeros((height, width))
    im = ax2.imshow(heatmap, cmap="hot", interpolation="nearest", origin="lower")
    plt.colorbar(im, ax=ax2)
    ax2.set_title("Population Density Heatmap")
    return fig, ax1, ax2, np.array(people), exit_point, walls, heatmap, im


def update(frame, scatter, people_container, exit_point, walls, ax1, ax2, heatmap, im):
    """
    update the simulation
    """
    people = people_container[0]
    if len(people) == 0:
        return scatter, im
    direction = exit_point - people
    distance = np.linalg.norm(direction, axis=1).reshape(-1, 1)
    direction /= np.where(distance == 0, 1, distance)  # Avoid division by zero
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
    new_positions = np.clip(new_positions, 0, max(ax1.get_xlim()[1], ax1.get_ylim()[1]))
    reached_exit = np.linalg.norm(new_positions - exit_point, axis=1) < 0.5
    people_container[0] = new_positions[~reached_exit]
    scatter.set_offsets(people_container[0])
    heatmap.fill(0)
    for person in people_container[0]:
        x, y = int(person[0]), int(person[1])
        if 0 <= x < heatmap.shape[1] and 0 <= y < heatmap.shape[0]:
            heatmap[y, x] += 1
    im.set_array(heatmap)
    return scatter, im


def run_simulation(layout_file):
    """
    run the simulation
    """
    layout = read_layout(layout_file)
    fig, ax1, ax2, people, exit_point, walls, heatmap, im = setup_simulation(layout)
    scatter = ax1.scatter(people[:, 0], people[:, 1], c="blue", s=10)
    people_container = [people]
    anim = FuncAnimation(
        fig,
        update,
        fargs=(scatter, people_container, exit_point, walls, ax1, ax2, heatmap, im),
        frames=200,
        interval=50,
        blit=False,
    )
    ax1.set_title("Movement simulation")
    fig.suptitle("Population density heatmap")
    plt.show()


# ----- SAMPLE EXECUTION CODE -----

if __name__ == "__main__":
    layout_file = "layout.txt"
    run_simulation(layout_file)
