module ChiyodaAccel

function aggregate_step_grids(
    width::Int,
    height::Int,
    positions::Matrix{Float64},
    densities::Vector{Float64},
    speeds::Vector{Float64},
)
    occupancy = zeros(Int, height, width)
    density_sum = zeros(Float64, height, width)
    speed_sum = zeros(Float64, height, width)

    for idx in 1:size(positions, 1)
        x = clamp(round(Int, positions[idx, 1]), 0, width - 1) + 1
        y = clamp(round(Int, positions[idx, 2]), 0, height - 1) + 1
        occupancy[y, x] += 1
        density_sum[y, x] += densities[idx]
        speed_sum[y, x] += speeds[idx]
    end

    density_grid = zeros(Float64, height, width)
    speed_grid = zeros(Float64, height, width)
    for y in 1:height
        for x in 1:width
            if occupancy[y, x] > 0
                density_grid[y, x] = density_sum[y, x] / occupancy[y, x]
                speed_grid[y, x] = speed_sum[y, x] / occupancy[y, x]
            end
        end
    end

    return occupancy, density_grid, speed_grid
end

function hazard_intensities(
    positions::Matrix{Float64},
    hazard_positions::Matrix{Float64},
    radii::Vector{Float64},
    severities::Vector{Float64},
)
    count = size(positions, 1)
    hazards = size(hazard_positions, 1)
    intensities = zeros(Float64, count)

    for idx in 1:count
        px = positions[idx, 1]
        py = positions[idx, 2]
        intensity = 0.0

        for h in 1:hazards
            hx = hazard_positions[h, 1]
            hy = hazard_positions[h, 2]
            radius = radii[h]
            severity = severities[h]
            distance = sqrt(((px - hx)^2) + ((py - hy)^2))

            if radius <= 1e-6
                if distance <= 0.75
                    intensity += severity
                end
            elseif distance <= radius
                intensity += severity * max(0.0, 1.0 - (distance / radius))
            end
        end

        intensities[idx] = intensity
    end

    return intensities
end

end
