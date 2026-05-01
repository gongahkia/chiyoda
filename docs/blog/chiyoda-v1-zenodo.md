# Publishing Chiyoda v1

I recently published the first version of Chiyoda on Zenodo:
[10.5281/zenodo.19905070](https://doi.org/10.5281/zenodo.19905070).

This is my first proper research paper, so getting it into a form that could be
read, cited, rebuilt, and criticized felt very different from shipping a normal
software project. Chiyoda started as code, but the paper forced me to ask a
harder question: what is the actual claim here, and what evidence do I have for
it?

The short version is that Chiyoda is a research toolkit for studying emergency
evacuation as an information problem, not just a routing problem. Most
evacuation systems implicitly treat communication as obviously helpful: tell
people more, give them better warnings, and the crowd should make better
decisions. I wanted to test the messier version of that story.

In Chiyoda, agents do not move through a perfect map of the world. They carry
beliefs about exits, hazards, congestion, and danger. They can observe their
surroundings, receive beacon messages, hear from responders, and pass distorted
local gossip to each other. They then route through the world they believe
exists, while the physical simulation still tracks crowding, bottlenecks,
hazard exposure, and evacuation outcomes.

That distinction is the part I find most interesting. A message can be correct
and still be risky. If a broadcast reduces uncertainty for everyone at once, it
can also synchronize too many people toward the same exit or through the same
hazard-adjacent route. So the paper does not ask only whether communication
helps. It asks when communication improves belief and safety together, and when
it creates harmful convergence.

The main experiments compare several communication policies: no intervention,
static beacons, global broadcasts, responder relay, entropy-targeted messages,
density-aware messages, exposure-aware messages, and bottleneck-avoidance
messages. Instead of ranking them only by evacuation count, I looked at metrics
that connect information and safety: belief entropy, belief accuracy, hazard
exposure, queue pressure, exit imbalance, information-safety efficiency, and a
harmful-convergence index.

Working through the paper made me realize how different research writing is
from writing project documentation. In a README, I can explain what the system
does and how to run it. In a paper, every strong sentence has to survive the
question: "compared to what, measured how, and supported by which result?" That
was uncomfortable at first, but it made the project much cleaner. I had to
separate the simulator from the study runner, the policy logic from the belief
updates, and the generated tables from hand-written claims.

The reproducibility work was also a big part of the experience. I did not want
the paper to be a PDF that happened to come from a private pile of scripts. The
repo now includes the simulation code, scenario YAML files, study commands,
generated figures, LaTeX source, and a reproducibility note that records how
the main artifacts were built. That took longer than I expected, but it is the
part that makes the Zenodo version feel like a real research artifact rather
than just a write-up.

I also added a bounded generated-guidance extension because I wanted to test
LLM-written evacuation messages without letting the paper become a vague "LLMs
for emergencies" claim. The generated messages are cached, replayable, and
validated before they can affect agent beliefs. The same safety metrics are
used for deterministic and generated guidance. That framing matters to me:
language is only useful in this setting if its downstream movement effects are
measured.

There are still clear limits. Chiyoda is a stylized simulator, not an
operational station-safety tool. The hazard physics are simplified, the
population model is synthetic, and the external validation is intentionally
narrow. I added a public Wuppertal bottleneck trajectory check to make the
trajectory-comparison pipeline more concrete, but it does not magically turn
the simulator into a calibrated real-world predictor.

Still, publishing v1 feels like a useful line in the sand. The project now has
a paper, a DOI, a reproducible study package, and a clearer research question:
how should emergency communication be evaluated when information can both help
people decide and accidentally coordinate them into danger?

That is the version of Chiyoda I am happiest with right now. Not a claim that
the simulator solves evacuation planning, but a framework for asking a sharper
question about communication, uncertainty, and safety.
