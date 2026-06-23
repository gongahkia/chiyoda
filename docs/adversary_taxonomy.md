# Adversary Taxonomy

Hostile channels use objective names from `data/adversary_incidents.yaml`. Scenario validation rejects any `hostile_channels[*].objective` outside that file.

## Objectives

| Objective | Meaning | Incident anchors |
| --- | --- | --- |
| `false-protective-action` | Misleading or incomplete guidance that steers people toward an unsafe or non-useful protective action. | Hawaii 2018 false missile alert, FCC report: https://docs.fcc.gov/public/attachments/DOC-350119A1.pdf; Rio Grande do Sul 2024 warning messages, Redes article PDF: https://seer.unisc.br/index.php/redes/article/view/19701/11870 |
| `threat-amplification` | Claims that overstate hazard presence, scale, certainty, or response failure. | Hawaii 2018 false missile alert, FCC report: https://docs.fcc.gov/public/attachments/DOC-350119A1.pdf; Hurricane Helene AI-generated imagery and false reports, NC DPS: https://www.ncdps.gov/news/press-releases/2024/10/04/guide-highlighting-trusted-information-sources |
| `authority-confusion` | Claims that blur, mimic, or misstate official emergency-management authority. | Fake FEMA inspectors/contractors after Hurricanes Debby, Helene, and Milton, FEMA: https://www.fema.gov/press-release/20250317/dont-get-scammed-be-aware-fake-fema-inspectors-and-contractors; Hawaii wildfire aid-confiscation rumor naming FEMA and Red Cross, FEMA: https://www.fema.gov/disaster/4724/rumor-response |
| `social-proof-poisoning` | Repeated peer, crowd, or social-media claims that make unsupported information look validated. | Hurricane Helene false reports on social media, NC DPS: https://www.ncdps.gov/news/press-releases/2024/10/04/guide-highlighting-trusted-information-sources; Hurricane Helene and Milton rumor response, FEMA: https://www.fema.gov/disaster/recover/rumor/hurricane-rumor-response |

## Model Mapping

`HostileChannelConfig.objective` is a strict enum. Unknown objective strings fail at config construction, and `validate-scenario` reports `unknown_hostile_objective` before a run.

The simulation effects remain intentionally small:

- `false-protective-action`, `authority-confusion`, and `social-proof-poisoning` inject exit claims.
- `threat-amplification` injects hazard claims.
- `authority-confusion` uses an official-looking source id.
- `social-proof-poisoning` targets high-credibility peers first.

LLM red-team references are kept separate from the incident anchors: TAMAS (`https://arxiv.org/abs/2511.05269`), AiTM (`https://arxiv.org/abs/2502.14847`), and MAST (`https://arxiv.org/abs/2508.03125`).
