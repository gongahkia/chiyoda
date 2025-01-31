import random
import numpy as np

def generate_layout(width, height):
    # Initialize the layout with empty spaces
    layout = np.full((height, width), '.')

    # Add outer walls
    layout[0, :] = 'X'
    layout[-1, :] = 'X'
    layout[:, 0] = 'X'
    layout[:, -1] = 'X'

    # Add logical internal walls
    add_internal_walls(layout)

    # Add exits
    add_exits(layout)

    # Add people
    add_people(layout)

    return layout

def add_internal_walls(layout):
    height, width = layout.shape
    
    # Add some vertical walls
    num_vertical_walls = random.randint(1, 3)
    for _ in range(num_vertical_walls):
        x = random.randint(width // 4, 3 * width // 4)
        wall_height = random.randint(height // 3, 2 * height // 3)
        start_y = random.randint(1, height - wall_height - 1)
        layout[start_y:start_y+wall_height, x] = 'X'
        
        # Add gaps in vertical walls
        num_gaps = random.randint(1, 3)
        for _ in range(num_gaps):
            gap_start = random.randint(start_y, start_y + wall_height - 2)
            gap_size = random.randint(1, 3)
            layout[gap_start:gap_start+gap_size, x] = '.'

    # Add some horizontal walls
    num_horizontal_walls = random.randint(1, 3)
    for _ in range(num_horizontal_walls):
        y = random.randint(height // 4, 3 * height // 4)
        wall_width = random.randint(width // 3, 2 * width // 3)
        start_x = random.randint(1, width - wall_width - 1)
        layout[y, start_x:start_x+wall_width] = 'X'
        
        # Add gaps in horizontal walls
        num_gaps = random.randint(1, 3)
        for _ in range(num_gaps):
            gap_start = random.randint(start_x, start_x + wall_width - 2)
            gap_size = random.randint(1, 3)
            layout[y, gap_start:gap_start+gap_size] = '.'

def add_exits(layout):
    num_exits = random.randint(1, 3)
    for _ in range(num_exits):
        side = random.choice(['top', 'bottom', 'left', 'right'])
        if side == 'top':
            x = random.randint(1, layout.shape[1] - 2)
            layout[0, x] = 'E'
        elif side == 'bottom':
            x = random.randint(1, layout.shape[1] - 2)
            layout[-1, x] = 'E'
        elif side == 'left':
            y = random.randint(1, layout.shape[0] - 2)
            layout[y, 0] = 'E'
        else:
            y = random.randint(1, layout.shape[0] - 2)
            layout[y, -1] = 'E'

def add_people(layout):
    num_people = random.randint(10, 30)
    empty_spaces = np.where(layout == '.')
    people_positions = random.sample(list(zip(empty_spaces[0], empty_spaces[1])), min(num_people, len(empty_spaces[0])))
    for y, x in people_positions:
        layout[y, x] = '@'

def save_layout(layout, filename):
    with open(filename, 'w') as f:
        for row in layout:
            f.write(''.join(row) + '\n')

if __name__ == "__main__":
    width = 30
    height = 20
    layout = generate_layout(width, height)
    save_layout(layout, 'generated_layout.txt')
    print("Layout generated and saved to 'generated_layout.txt'")
