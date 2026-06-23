module ChiyodaAccel

const TAU = 0.5
const A_AGENT = 2.1
const B_AGENT = 0.3
const A_WALL = 5.0
const B_WALL = 0.2
const COUNTER_FLOW_K = 1.5
const VISUAL_RANGE = 0.0
const VISUAL_FIELD_COSINE = cos(180.0 * pi / 180.0)
const REAR_REPULSION_WEIGHT = 1.0
const COUNTER_FLOW_AVOIDANCE_K = 0.0
const COUNTER_FLOW_AVOIDANCE_RANGE = 2.0

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
        desired_speed = sqrt(sum(desired_velocities[idx, :] .^ 2))

        for n in 1:neighbor_counts[idx]
            delta = current_positions[idx, :] - neighbor_positions[idx, n, :]
            dist = sqrt(sum(delta .^ 2)) + 1e-6
            if dist < 3.0
                if VISUAL_RANGE > 0.0 && dist > VISUAL_RANGE
                    continue
                end
                visual_weight = 1.0
                if desired_speed > 1e-6
                    to_neighbor = -delta ./ dist
                    cos_angle = sum(desired_velocities[idx, :] .* to_neighbor) / desired_speed
                    if cos_angle < VISUAL_FIELD_COSINE
                        visual_weight = REAR_REPULSION_WEIGHT
                    end
                end
                n_hat = delta ./ dist
                f_total .+= visual_weight * A_AGENT * exp((0.6 - dist) / B_AGENT) .* n_hat

                if counter_flow
                    n_vel = neighbor_velocities[idx, n, :]
                    dot_value = sum(desired_velocities[idx, :] .* n_vel)
                    n_speed = sqrt(sum(n_vel .^ 2))
                    if dot_value < 0 && n_speed > 0.1
                        tangent = zeros(Float64, dim)
                        tangent[1] = -n_hat[2]
                        tangent[2] = n_hat[1]
                        f_total .+= (
                            visual_weight
                            *
                            COUNTER_FLOW_K
                            * abs(dot_value)
                            * sign(sum(tangent .* desired_velocities[idx, :]))
                        ) .* tangent
                        if desired_speed > 1e-6 && COUNTER_FLOW_AVOIDANCE_K > 0.0
                            lateral_axis = zeros(Float64, dim)
                            lateral_axis[1] = -desired_velocities[idx, 2] / desired_speed
                            lateral_axis[2] = desired_velocities[idx, 1] / desired_speed
                            lateral_offset = sum(delta .* lateral_axis)
                            lateral_sign = 1.0
                            if abs(lateral_offset) > 1e-6
                                lateral_sign = sign(lateral_offset)
                            end
                            approach = min(1.0, -dot_value / (desired_speed * n_speed))
                            f_total .+= (
                                visual_weight
                                * COUNTER_FLOW_AVOIDANCE_K
                                * approach
                                * exp((COUNTER_FLOW_AVOIDANCE_RANGE - dist) / COUNTER_FLOW_AVOIDANCE_RANGE)
                                * lateral_sign
                            ) .* lateral_axis
                        end
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
