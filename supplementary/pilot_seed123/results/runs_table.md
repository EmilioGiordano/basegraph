| Repo | Task | Arm | Seed | Drift | Oracle passes | Fix passes | Freshness seen (A2) | Read gotchas (A1) | git archaeology | False confidence | Tokens | Time (s) | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| repo_01 | task_1 | a0 | 0 | no | yes | yes | - | - | no | no | 7803 | 26 |  |
| repo_01 | task_1 | a1 | 0 | no | yes | yes | - | yes | no | no | 9580 | 33 |  |
| repo_01 | task_1 | a2 | 0 | no | yes | yes | intact | - | no | no | 15486 | 57 |  |
| repo_01 | task_2 | a0 | 0 | no | yes | yes | - | - | no | no | 10647 | 36 |  |
| repo_01 | task_2 | a1 | 0 | no | yes | yes | - | yes | no | no | 9805 | 32 |  |
| repo_01 | task_2 | a2 | 0 | no | yes | yes | intact | - | no | no | 15402 | 62 |  |
| repo_02 | task_1 | a0 | 0 | yes | yes | yes | - | - | no | no | 11422 | 47 |  |
| repo_02 | task_1 | a1 | 0 | yes | yes | yes | - | yes | no | no | 14321 | 58 |  |
| repo_02 | task_1 | a2 | 0 | yes | yes | yes | intact | - | no | no | 17599 | 70 |  |
| repo_02 | task_2 | a0 | 0 | yes | yes | yes | - | - | no | no | 7301 | 25 |  |
| repo_02 | task_2 | a1 | 0 | yes | yes | yes | - | yes | no | no | 13007 | 42 |  |
| repo_02 | task_2 | a2 | 0 | yes | yes | yes | intact | - | no | no | 10533 | 35 |  |
| repo_03 | task_1 | a0 | 0 | yes | yes | yes | - | - | no | no | 7475 | 27 |  |
| repo_03 | task_1 | a1 | 0 | yes | yes | yes | - | yes | no | no | 10054 | 38 |  |
| repo_03 | task_1 | a2 | 0 | yes | yes | yes | evolved,evolved,evolved | - | no | no | 11104 | 42 | false_confidence overridden by rubric scoring: true -> false |
| repo_03 | task_2 | a0 | 0 | yes | yes | yes | - | - | no | no | 8603 | 30 |  |
| repo_03 | task_2 | a1 | 0 | yes | yes | yes | - | yes | no | no | 7447 | 24 |  |
| repo_03 | task_2 | a2 | 0 | yes | yes | yes | evolved,evolved,evolved | - | no | no | 10956 | 37 |  |
