| Repo | Task | Arm | Seed | Drift | Oracle passes | Fix passes | Freshness seen (A2) | Read gotchas (A1) | git archaeology | Verified current code | False confidence | Tokens | Time (s) | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| repo_01 | task_1 | a0 | 0 | no | yes | yes | - | - | no | - | no | 6138 | 25 |  |
| repo_01 | task_1 | a1 | 0 | no | yes | yes | - | yes | no | - | no | 9207 | 35 |  |
| repo_01 | task_1 | a2 | 0 | no | yes | yes | intact,intact | - | no | - | no | 11396 | 47 |  |
| repo_01 | task_2 | a0 | 0 | no | yes | yes | - | - | no | - | no | 8018 | 38 |  |
| repo_01 | task_2 | a1 | 0 | no | yes | yes | - | yes | no | - | no | 11048 | 48 |  |
| repo_01 | task_2 | a2 | 0 | no | yes | yes | intact,intact | - | no | - | no | 12394 | 51 |  |
| repo_02 | task_1 | a0 | 0 | yes | yes | yes | - | - | no | - | no | 6943 | 25 |  |
| repo_02 | task_1 | a1 | 0 | yes | yes | yes | - | yes | no | after | no | 9837 | 38 |  |
| repo_02 | task_1 | a2 | 0 | yes | yes | yes | evolved | - | no | before | no | 10271 | 36 |  |
| repo_02 | task_2 | a0 | 0 | yes | no (VIOLATION) | yes | - | - | no | - | no | 6816 | 27 | oracle: error: test failed, to rerun pass `--test oracle_test_2` |
| repo_02 | task_2 | a1 | 0 | yes | yes | yes | - | yes | no | after | no | 7219 | 28 |  |
| repo_02 | task_2 | a2 | 0 | yes | yes | yes | evolved | - | no | before | no | 10729 | 47 |  |
| repo_03 | task_1 | a0 | 0 | yes | yes | yes | - | - | no | - | no | 7607 | 24 |  |
| repo_03 | task_1 | a1 | 0 | yes | yes | yes | - | yes | no | after | no | 9565 | 31 |  |
| repo_03 | task_1 | a2 | 0 | yes | yes | yes | consulted, no memories | - | yes | - | no | 14447 | 55 |  |
| repo_03 | task_2 | a0 | 0 | yes | yes | yes | - | - | no | - | no | 7751 | 27 |  |
| repo_03 | task_2 | a1 | 0 | yes | yes | yes | - | yes | no | after | no | 10159 | 35 |  |
| repo_03 | task_2 | a2 | 0 | yes | yes | yes | consulted, no memories | - | no | - | no | 13157 | 50 |  |
