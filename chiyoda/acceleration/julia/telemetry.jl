module ChiyodaAccel

const TAU = 0.5
const A_AGENT = 2.1
const B_AGENT = 0.3
const A_WALL = 5.0
const B_WALL = 0.2
const COUNTER_FLOW_K = 1.5

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

function social_force_steps(
    current_positions::Matrix{Float64},
    desired_velocities::Matrix{Float64},
    current_velocities::Matrix{Float64},
    neighbor_positions::Array{Float64, 3},
    neighbor_counts::Vector{Int},
    neighbor_velocities::Array{Float64, 3},
    walls::Matrix{Float64},
    dt::Float64,
    counter_flow::Bool,
)
    count = size(current_positions, 1)
    dim = size(current_positions, 2)
    displacements = zeros(Float64, count, dim)

    for idx in 1:count
        f_total = (
            desired_velocities[idx, :] - current_velocities[idx, :]
        ) ./ TAU

        for n in 1:neighbor_counts[idx]
            delta = current_positions[idx, :] - neighbor_positions[idx, n, :]
            dist = sqrt(sum(delta .^ 2)) + 1e-6
            if dist < 3.0
                n_hat = delta ./ dist
                f_total .+= A_AGENT * exp((0.6 - dist) / B_AGENT) .* n_hat

                if counter_flow
                    n_vel = neighbor_velocities[idx, n, :]
                    dot_value = sum(desired_velocities[idx, :] .* n_vel)
                    if dot_value < 0 && sqrt(sum(n_vel .^ 2)) > 0.1
                        tangent = zeros(Float64, dim)
                        tangent[1] = -n_hat[2]
                        tangent[2] = n_hat[1]
                        f_total .+= (
                            COUNTER_FLOW_K
                            * abs(dot_value)
                            * sign(sum(tangent .* desired_velocities[idx, :]))
                        ) .* tangent
                    end
                end
            end
        end

        for w in 1:size(walls, 1)
            delta = current_positions[idx, :] - walls[w, :]
            dist = sqrt(sum(delta .^ 2)) + 1e-6
            if dist < 2.0
                n_hat = delta ./ dist
                f_total .+= A_WALL * exp((0.3 - dist) / B_WALL) .* n_hat
            end
        end

        new_velocity = current_velocities[idx, :] + f_total .* dt
        max_speed = max(sqrt(sum(desired_velocities[idx, :] .^ 2)) * 1.5, 0.5)
        speed = sqrt(sum(new_velocity .^ 2))
        if speed > max_speed
            new_velocity = new_velocity ./ speed .* max_speed
        end
        displacements[idx, :] = new_velocity .* dt
    end

    return displacements
end

end
