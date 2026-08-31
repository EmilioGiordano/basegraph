| Repo | Task | Arm | Seed | Drift | Oracle passes | Fix passes | Freshness seen (A2) | Read gotchas (A1) | git archaeology | Verified current code | False confidence | Tokens | Time (s) | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| repo_01 | task_1 | a0 | 0 | no | yes | yes | - | - | no | - | no | 6138 | 25 |  |
| repo_01 | task_2 | a0 | 0 | no | yes | yes | - | - | no | - | no | 8018 | 38 |  |
| repo_02 | task_1 | a0 | 0 | yes | yes | yes | - | - | no | - | no | 6943 | 25 |  |
| repo_02 | task_2 | a0 | 0 | yes | no (VIOLATION) | yes | - | - | no | - | no | 6816 | 27 | oracle: error: test failed, to rerun pass `--test oracle_test_2` |
| repo_03 | task_1 | a0 | 0 | yes | yes | yes | - | - | no | - | no | 7607 | 24 |  |
| repo_03 | task_2 | a0 | 0 | yes | yes | yes | - | - | no | - | no | 7751 | 27 |  |
