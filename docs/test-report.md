# git-wt test report

- Version: `git-wt 2.0.0`
- Build: `release`
- Date: `2026-07-26 09:14:35 PST`

## Results

| | Tag | Test | Command | Failure |
|---|---|---|---|---|
| ✅ | HAPPY | list with no args shows main | `` git-wt list `` |  |
| ✅ | HAPPY | list shows main | `` git-wt list `` |  |
| ✅ | HAPPY | ls alias | `` git-wt ls `` |  |
| ✅ | HAPPY | list no-match errors | `` git-wt list zzz `` |  |
| ✅ | HAPPY | add existing local branch | `` git-wt add feature/login `` |  |
| ✅ | HAPPY | list shows new worktree | `` git-wt list `` |  |
| ✅ | HAPPY | list filter keeps index | `` git-wt list logi `` |  |
| ✅ | HAPPY | list --col branch only | `` git-wt list --col 2 logi `` |  |
| ✅ | HAPPY | list --col id+branch | `` git-wt list --col 1,2 logi `` |  |
| ✅ | HAPPY | list --col reorder | `` git-wt list --col 2,1 logi `` |  |
| ✅ | UNHAPPY | list --col bad number | `` git-wt list --col 11 `` |  |
| ✅ | UNHAPPY | list --col non-numeric | `` git-wt list --col x `` |  |
| ✅ | HAPPY | bare --col means list | `` git-wt list --col 2 `` |  |
| ✅ | HAPPY | bare -c short flag means list | `` git-wt list -c 1,2 `` |  |
| ✅ | HAPPY | add --name suffix | `` git-wt add feature/logout --name review `` |  |
| ✅ | HAPPY | add --dirname whole leaf | `` git-wt add feature/api --dirname scratch2 `` |  |
| ✅ | HAPPY | add tracks remote-only | `` git-wt add remote-only `` |  |
| ✅ | HAPPY | add --dirname as path | `` git-wt add pathtest --dirname sub/deep `` |  |
| ✅ | HAPPY | add --from a ref (new branch) | `` git-wt add newfrom --from feature/login --dirname ff1 `` |  |
| ✅ | UNHAPPY | add dup dir refused | `` git-wt add feature/login `` |  |
| ✅ | UNHAPPY | add name+dirname conflict | `` git-wt add x -n a --dirname b `` |  |
| ✅ | UNHAPPY | add --name empty | `` git-wt add x -n '' `` |  |
| ✅ | UNHAPPY | add --from needs ref | `` git-wt add x --from `` |  |
| ✅ | HAPPY | add new branch declined | `` git-wt add nope --dirname np1 `` |  |
| ✅ | HAPPY | add --stay accepted | `` git-wt add staybr --dirname stay1 --stay `` |  |
| ✅ | UNHAPPY | picker lists checked-out sep | `` git-wt add `` |  |
| ✅ | UNHAPPY | picker shows a checked-out br | `` git-wt add `` |  |
| ✅ | UNHAPPY | picker errors when all checked out | `` git-wt add  # in a fully-checked-out repo `` |  |
| ✅ | HAPPY | add --from base commit matches ref | `` git rev-parse HEAD  # in ff1 `` |  |
| ✅ | HAPPY | switch N prints path | `` git-wt switch 1 `` |  |
| ✅ | HAPPY | path N prints path | `` git-wt path 1 `` |  |
| ✅ | UNHAPPY | switch N too many args | `` git-wt switch 1 path `` |  |
| ✅ | UNHAPPY | index 0 errors | `` git-wt switch 0 `` |  |
| ✅ | UNHAPPY | index over range errors | `` git-wt switch 99 `` |  |
| ✅ | UNHAPPY | extra arg after switch | `` git-wt switch 1 bogus `` |  |
| ✅ | UNHAPPY | flag on bare switch | `` git-wt switch 1 -n x `` |  |
| ✅ | UNHAPPY | long flag on bare switch | `` git-wt switch 1 --stat `` |  |
| ✅ | UNHAPPY | retired show verb rejected | `` git-wt show 1 `` |  |
| ✅ | UNHAPPY | legacy remove order rejected | `` git-wt 1 remove `` |  |
| ✅ | UNHAPPY | bare branch name rejected | `` git-wt feat/x `` |  |
| ✅ | UNHAPPY | typo verb rejected | `` git-wt lsit `` |  |
| ✅ | HAPPY | diff --name-only shows adds | `` git-wt diff 3 --name-only `` |  |
| ✅ | HAPPY | diff .. keeps main-only file | `` git-wt diff 3 .. --name-only `` |  |
| ✅ | HAPPY | diff (default) hides main-only file | `` git-wt diff 3 --name-only `` |  |
| ✅ | HAPPY | diff ... hides main-only file | `` git-wt diff 3 ... --name-only `` |  |
| ✅ | HAPPY | diff --stat | `` git-wt diff 3 --stat `` |  |
| ✅ | HAPPY | diff --name-status | `` git-wt diff 3 --name-status `` |  |
| ✅ | HAPPY | diff -p limits to the path | `` git-wt diff 3 -p onlylogin.txt --name-only `` |  |
| ✅ | UNHAPPY | diff -- is retired | `` git-wt diff 3 --name-only -- onlylogin.txt `` |  |
| ✅ | UNHAPPY | bare path points at -p | `` git-wt diff 3 onlylogin.txt `` |  |
| ✅ | HAPPY | diff -A keeps main-only file | `` git-wt diff 3 -A .. --name-only `` |  |
| ✅ | HAPPY | diff -B keeps login-only file | `` git-wt diff 3 -B .. --name-only `` |  |
| ✅ | HAPPY | diff -A names the side | `` git-wt diff 3 -A --hunks .. `` |  |
| ✅ | UNHAPPY | diff -A -B cannot combine | `` git-wt diff 3 -A -B `` |  |
| ✅ | HAPPY | diff --a-only long form | `` git-wt diff 3 --a-only .. --name-only `` |  |
| ✅ | UNHAPPY | diff needs a source | `` git-wt diff `` |  |
| ✅ | UNHAPPY | diff self via default dest | `` git-wt diff 1 `` |  |
| ✅ | UNHAPPY | diff old form errors | `` git-wt switch 1 diff 3 `` |  |
| ✅ | UNHAPPY | diff non-numeric target | `` git-wt diff x `` |  |
| ✅ | UNHAPPY | diff bad index errors | `` git-wt diff 99 `` |  |
| ✅ | UNHAPPY | diff -d takes exactly one target | `` git-wt diff 3 -d 1,3 `` |  |
| ✅ | UNHAPPY | diff -s takes exactly one source | `` git-wt diff -s 1,3 `` |  |
| ✅ | UNHAPPY | diff rejects a source list | `` git-wt diff 1,3 `` |  |
| ✅ | UNHAPPY | diff source given twice | `` git-wt diff 3 -s 3 `` |  |
| ✅ | UNHAPPY | diff rejects other git flags | `` git-wt diff 3 -w `` |  |
| ✅ | HAPPY | diff warns on dirty worktree | `` git-wt diff 3 --name-only `` |  |
| ✅ | UNHAPPY | diff outside a worktree needs -d | `` git-wt diff 3  # cwd outside any worktree `` |  |
| ✅ | HAPPY | diff -d from outside works | `` git-wt diff 3 -d 1 --name-only  # cwd outside any worktree `` |  |
| ✅ | HAPPY | commits single target | `` git-wt commits 3 `` |  |
| ✅ | HAPPY | commits single drops the legend | `` git-wt commits 3 `` |  |
| ✅ | HAPPY | commits single takes flags | `` git-wt commits 3 --author Test `` |  |
| ✅ | HAPPY | commits single self is a log | `` git-wt commits 1 `` |  |
| ✅ | HAPPY | commits heads the columns | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits lists its own side | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits anchors on the first | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits --union adds the rest | `` git-wt commits 1,3 --union `` |  |
| ✅ | HAPPY | commits heads the author col | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits names the author | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits heads the subject col | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits dates the rows | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits --date-human | `` git-wt commits 1,3 --date-human `` |  |
| ✅ | HAPPY | commits --time | `` git-wt commits 1,3 --time `` |  |
| ✅ | HAPPY | commits --time is 24h | `` git-wt commits 1,3 --time `` |  |
| ✅ | HAPPY | commits human + time | `` git-wt commits 1,3 --date-human --time `` |  |
| ✅ | HAPPY | commits ISO round-trips | `` git-wt commits 1,3 --date-since 2026-07-26 `` |  |
| ✅ | HAPPY | commits default shows slice | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits --all shows full log | `` git-wt commits 1,3 --all `` |  |
| ✅ | UNHAPPY | commits --all vs --union | `` git-wt commits 1,3 --all --union `` |  |
| ✅ | HAPPY | commits -a aliases --all | `` git-wt commits 1,3 -a `` |  |
| ✅ | HAPPY | commits -f aliases --files | `` git-wt commits 1,3 -a -f `` |  |
| ✅ | HAPPY | commits -af bundles both | `` git-wt commits 1,3 -af `` |  |
| ✅ | HAPPY | commits -fa order-free | `` git-wt commits 1,3 -fa `` |  |
| ✅ | HAPPY | commits -fn takes a value | `` git-wt commits 1,3 -afn 20 `` |  |
| ✅ | UNHAPPY | commits -nf refused | `` git-wt commits 1,3 -nf 20 `` |  |
| ✅ | UNHAPPY | commits -zy names the first | `` git-wt commits 1,3 -zy `` |  |
| ✅ | HAPPY | commits default drops shared root | `` git-wt commits 1,3  # no init row `` |  |
| ✅ | HAPPY | commits leaves foreign cell | `` git-wt commits 1,3  # mainside unchecked on login `` |  |
| ✅ | HAPPY | commits takes three worktrees | `` git-wt commits 1,3,5 `` |  |
| ✅ | HAPPY | commits --topo keeps the rows | `` git-wt commits 1,3 --topo `` |  |
| ✅ | HAPPY | commits --topo-order spelling | `` git-wt commits 1,3 --union --topo-order `` |  |
| ✅ | HAPPY | commits --topo same row set | `` git-wt commits 1,3 --union --topo  # 2 rows either way `` |  |
| ✅ | HAPPY | commits -n 1 caps the rows | `` git-wt commits 1,3 -n 1 `` |  |
| ✅ | HAPPY | commits --limit=1 caps the rows | `` git-wt commits 1,3 --limit=1 `` |  |
| ✅ | HAPPY | commits align past an emoji | `` git-wt commits 1,3  # emoji subject, marks hold `` |  |
| ✅ | HAPPY | commits --date exact day | `` git-wt commits 1,3 --date 2026-07-26 `` |  |
| ✅ | HAPPY | commits --date other day | `` git-wt commits 1,3 --date 2026-07-27 `` |  |
| ✅ | HAPPY | commits --date-since today | `` git-wt commits 1,3 --date-since 2026-07-26 `` |  |
| ✅ | HAPPY | commits --date-until today | `` git-wt commits 1,3 --date-until 2026-07-26 `` |  |
| ✅ | HAPPY | commits date range brackets | `` git-wt commits 1,3 --date-since 2026-07-25 --date-until 2026-07-27 `` |  |
| ✅ | HAPPY | commits --date-since tomorrow | `` git-wt commits 1,3 --date-since 2026-07-27 `` |  |
| ✅ | HAPPY | commits --date-until yesterday | `` git-wt commits 1,3 --date-until 2026-07-25 `` |  |
| ✅ | UNHAPPY | commits --date rejects >= | `` git-wt commits 1,3 --date >=2026-07-26 `` |  |
| ✅ | UNHAPPY | commits --date rejects <= | `` git-wt commits 1,3 --date <=2026-07-26 `` |  |
| ✅ | UNHAPPY | commits --date rejects = | `` git-wt commits 1,3 --date =2026-07-26 `` |  |
| ✅ | UNHAPPY | commits --date bad shape | `` git-wt commits 1,3 --date 2026-1-1 `` |  |
| ✅ | UNHAPPY | commits --date impossible | `` git-wt commits 1,3 --date 2026-13-01 `` |  |
| ✅ | UNHAPPY | commits --date needs a value | `` git-wt commits 1,3 --date `` |  |
| ✅ | UNHAPPY | commits --date eaten by shell | `` git-wt commits 1,3 --date >= `` |  |
| ✅ | HAPPY | --commits implies --all | `` git-wt commits 1,3 --commits 97883ae `` |  |
| ✅ | HAPPY | --date implies --all | `` git-wt commits 1,3 --date 2026-07-26 `` |  |
| ✅ | HAPPY | --date-since implies --all | `` git-wt commits 1,3 --date-since 2026-07-25 `` |  |
| ✅ | HAPPY | --date-until keeps the slice | `` git-wt commits 1,3 --date-until 2026-07-27  # no init row `` |  |
| ✅ | HAPPY | --date-until --all widens | `` git-wt commits 1,3 --date-until 2026-07-27 --all `` |  |
| ✅ | HAPPY | a date range implies --all | `` git-wt commits 1,3 --date-since 2026-07-25 --date-until 2026-07-27 `` |  |
| ✅ | HAPPY | empty filter names the slice | `` git-wt commits 1,3 --date-until 2026-07-25 `` |  |
| ✅ | HAPPY | --commit-until keeps nothing | `` git-wt commits 1,2 --commit-until 3ee86a1 `` |  |
| ✅ | HAPPY | --commit-until --all reaches back | `` git-wt commits 1,2 --commit-until 3ee86a1 --all `` |  |
| ✅ | HAPPY | --author keeps the default slice | `` git-wt commits 1,3 --author Test  # no init row `` |  |
| ✅ | HAPPY | --author --all is the full log | `` git-wt commits 1,3 --author Test --all `` |  |
| ✅ | HAPPY | --message keeps the default slice | `` git-wt commits 1,3 --message init  # no init row `` |  |
| ✅ | HAPPY | --message --all is the full log | `` git-wt commits 1,3 --message init --all `` |  |
| ✅ | HAPPY | --union survives a date filter | `` git-wt commits 1,3 --union --date 2026-07-26 `` |  |
| ✅ | HAPPY | --commits lights its sha | `` git-wt commits 1,$didx --commits ff3a201 `` |  |
| ✅ | HAPPY | --date lights the date | `` git-wt commits 1,$didx --date 2026-07-26 `` |  |
| ✅ | HAPPY | --author lights the author | `` git-wt commits 1,$didx --author Test `` |  |
| ✅ | HAPPY | --commit-until leaves the date dim | `` git-wt commits 1,3 --commit-until ff3a201 `` |  |
| ✅ | HAPPY | --commit-until lights its sha | `` git-wt commits 1,3 --commit-until ff3a201 `` |  |
| ✅ | HAPPY | --message lights the matched word | `` git-wt commits 1,$didx --message mainside `` |  |
| ✅ | HAPPY | --path lights the path | `` git-wt commits 1,$didx --path onlymain.txt --all `` |  |
| ✅ | HAPPY | no filter lights nothing | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits --commit-since keeps its own | `` git-wt commits 1,3 --commit-since ff3a201 `` |  |
| ✅ | HAPPY | commits --commit-until keeps its own | `` git-wt commits 1,3 --commit-until ff3a201 `` |  |
| ✅ | HAPPY | commit bound is a date, not ancestry | `` git-wt commits 1,3 --union --commit-until ff3a201 `` |  |
| ✅ | UNHAPPY | commits --commit-since bad commit | `` git-wt commits 1,3 --commit-since zzz9 `` |  |
| ✅ | UNHAPPY | commits --commit-until needs a value | `` git-wt commits 1,3 --commit-until `` |  |
| ✅ | HAPPY | commits --commits one sha | `` git-wt commits 1,3 --commits ff3a201 `` |  |
| ✅ | HAPPY | commits -i short flag | `` git-wt commits 1,3 -i ff3a201 `` |  |
| ✅ | HAPPY | commits --commits bundled -ai | `` git-wt commits 1,3 -ai ff3a201 `` |  |
| ✅ | UNHAPPY | commits --commits bad sha | `` git-wt commits 1,3 --commits zzz9 `` |  |
| ✅ | UNHAPPY | commits --commits empty id | `` git-wt commits 1,3 --commits a,,b `` |  |
| ✅ | HAPPY | commits --commits keeps only those | `` git-wt commits 1,3 --all --commits ff3a201  # 1 of 3 rows `` |  |
| ✅ | HAPPY | commits --merges shows them | `` git-wt commits 1,3 --merges `` |  |
| ✅ | HAPPY | commits drops merges | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits hides merges by default | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits default keeps work | `` git-wt commits 1,3 `` |  |
| ✅ | HAPPY | commits --reverse flips rows | `` git-wt commits 1,3 -n 2 --reverse `` |  |
| ✅ | HAPPY | commits --oldest-first alias | `` git-wt commits 1,3 --oldest-first `` |  |
| ✅ | HAPPY | commits --md names the file | `` git-wt commits 1,3 --md /tmp/git-wt-test.U4TZ... `` |  |
| ✅ | HAPPY | commits --md=PATH spelling | `` git-wt commits 1,3 --md=/tmp/git-wt-test... `` |  |
| ✅ | HAPPY | commits --md writes a table | `` git-wt commits 1,3 --md /tmp/git-wt-test.U4TZ... `` |  |
| ✅ | HAPPY | commits --md escapes pipes | `` git-wt commits 1,3 --md /tmp/git-wt-test.U4TZ... `` |  |
| ✅ | HAPPY | commits --md default name | `` git-wt commits 1,3 --md --topo  # commits_<stamp>.md `` |  |
| ✅ | UNHAPPY | commits --md bad dir errors | `` git-wt commits 1,3 --md /nope/nope/x.md `` |  |
| ✅ | HAPPY | commits marks a cherry-pick | `` git-wt commits 1,3 --union  # ≈ = same patch, other sha `` |  |
| ✅ | HAPPY | commits --no-cherry drops ≈ | `` git-wt commits 1,3 --union --no-cherry `` |  |
| ✅ | HAPPY | commits --no-cherry is plain | `` git-wt commits 1,3 --union --no-cherry  # no ≈ without the walk `` |  |
| ✅ | HAPPY | commits leaves unpicked work | `` git-wt commits 1,3 --union  # loginside: not on main, not ≈ `` |  |
| ✅ | HAPPY | commits --author exact | `` git-wt commits 1,3 --author Test `` |  |
| ✅ | HAPPY | commits --author fuzzy | `` git-wt commits 1,3 --author tst `` |  |
| ✅ | HAPPY | commits --author case-folds | `` git-wt commits 1,3 --author TEST `` |  |
| ✅ | HAPPY | commits --author no match | `` git-wt commits 1,3 --author zzzz `` |  |
| ✅ | UNHAPPY | commits --author needs a name | `` git-wt commits 1,3 --author `` |  |
| ✅ | HAPPY | commits --message subject | `` git-wt commits 1,3 --message mainside `` |  |
| ✅ | HAPPY | commits --message case-folds | `` git-wt commits 1,3 --message MAINSIDE `` |  |
| ✅ | HAPPY | commits -m short form | `` git-wt commits 1,3 -m mainside `` |  |
| ✅ | HAPPY | commits --message no match | `` git-wt commits 1,3 --message zzzz `` |  |
| ✅ | UNHAPPY | commits --message needs a term | `` git-wt commits 1,3 --message `` |  |
| ✅ | HAPPY | commits --message empty is a no-op | `` git-wt commits 1,3 --message '' `` |  |
| ✅ | UNHAPPY | commits --path needs a term | `` git-wt commits 1,3 --path `` |  |
| ✅ | HAPPY | commits --path trims block | `` git-wt commits 1,3 --path blockmatch --all `` |  |
| ✅ | HAPPY | commits --path cuts the rest | `` git-wt commits 1,3 --path blockmatch --all `` |  |
| ✅ | HAPPY | commits --all-files widens | `` git-wt commits 1,3 --path blockmatch --all --all-files `` |  |
| ✅ | HAPPY | commits --all-files keeps match | `` git-wt commits 1,3 --path blockmatch --all --all-files `` |  |
| ✅ | UNHAPPY | commits --all-files alone errors | `` git-wt commits 1,3 --all-files `` |  |
| ✅ | UNHAPPY | commits rejects a dup target | `` git-wt commits 1,1 `` |  |
| ✅ | UNHAPPY | commits bad index errors | `` git-wt commits 1,99 `` |  |
| ✅ | UNHAPPY | commits -n needs a count | `` git-wt commits 1,3 -n `` |  |
| ✅ | UNHAPPY | commits -n 0 errors | `` git-wt commits 1,3 -n 0 `` |  |
| ✅ | UNHAPPY | commits -n non-numeric | `` git-wt commits 1,3 -n x `` |  |
| ✅ | HAPPY | bare commits uses current | `` git-wt commits `` |  |
| ✅ | HAPPY | ref diff blind on same commit | `` git-wt diff 4 --name-only  # empty, as designed `` |  |
| ✅ | HAPPY | live sees uncommitted edit | `` git-wt diff 4 --live --name-only `` |  |
| ✅ | HAPPY | live sees untracked as add | `` git-wt diff 4 --live --name-status `` |  |
| ✅ | HAPPY | live counts hunks | `` git-wt diff 4 --live `` |  |
| ✅ | HAPPY | live summary counts lines | `` git-wt diff 4 --live `` |  |
| ✅ | HAPPY | live hunks show line numbers | `` git-wt diff 4 --live --hunks `` |  |
| ✅ | HAPPY | live --stat still works | `` git-wt diff 4 --live --stat `` |  |
| ✅ | HAPPY | live suppresses dirty warn | `` git-wt diff 4 --live --name-only `` |  |
| ✅ | HAPPY | live -B keeps side-only add | `` git-wt diff 4 --live -B --name-only `` |  |
| ✅ | HAPPY | live -A empty says so | `` git-wt diff 4 --live -A --name-only `` |  |
| ✅ | HAPPY | live honors .gitignore | `` git-wt diff 4 --live --name-only `` |  |
| ✅ | HAPPY | live -p limits to the path | `` git-wt diff 4 --live -p shared.txt --name-only `` |  |
| ✅ | HAPPY | live reports on-disk delete | `` git-wt diff 4 --live --name-status `` |  |
| ✅ | HAPPY | live delete is not an add | `` git-wt diff 4 --live --hunks `` |  |
| ✅ | HAPPY | live identical is empty out | `` git-wt diff 4 --live -p .gitignore `` |  |
| ✅ | HAPPY | live identical says so once | `` git-wt diff 4 --live -p .gitignore --name-only `` |  |
| ✅ | UNHAPPY | live bad flag rejected | `` git-wt diff 4 --live -w `` |  |
| ✅ | UNHAPPY | bare 'live' word rejected | `` git-wt diff 4 live --name-only `` |  |
| ✅ | UNHAPPY | bare 'hunks' word rejected | `` git-wt diff 3 hunks `` |  |
| ✅ | UNHAPPY | live -- is retired | `` git-wt diff 4 --live --name-only -- shared.txt `` |  |
| ✅ | UNHAPPY | bare path after --live | `` git-wt diff 4 --live --name-only shared.txt `` |  |
| ✅ | HAPPY | -p limits the diff | `` git-wt diff 4 --live -p shared.txt --name-only `` |  |
| ✅ | HAPPY | --path long form | `` git-wt diff 4 --live --path shared.txt --name-only `` |  |
| ✅ | HAPPY | -p takes a comma list | `` git-wt diff 4 --live -p shared.txt,untracked.txt --name-only `` |  |
| ✅ | UNHAPPY | -p checks the path too | `` git-wt diff 4 --live -p nope/ --name-only `` |  |
| ✅ | UNHAPPY | -p empty element errors | `` git-wt diff 4 --live -p shared.txt, --name-only `` |  |
| ✅ | UNHAPPY | -p with a retired -- too | `` git-wt diff 4 --live -p shared.txt --name-only -- shared.txt `` |  |
| ✅ | HAPPY | glob path matches | `` git-wt diff 4 --live -p *.txt --name-only `` |  |
| ✅ | UNHAPPY | glob with no hit errors | `` git-wt diff 4 --live -p *.zzz --name-only `` |  |
| ✅ | UNHAPPY | live rejects .. range | `` git-wt diff 4 --live .. `` |  |
| ✅ | UNHAPPY | live rejects ... range | `` git-wt diff 4 --live ... `` |  |
| ✅ | UNHAPPY | hunks rejects --stat | `` git-wt diff 4 --hunks --stat `` |  |
| ✅ | HAPPY | hunks works without live | `` git-wt diff 3 --hunks `` |  |
| ✅ | HAPPY | list --files shows edit | `` git-wt list --files `` |  |
| ✅ | HAPPY | list --files counts lines | `` git-wt list --files `` |  |
| ✅ | HAPPY | list --files shows untracked | `` git-wt list --files `` |  |
| ✅ | HAPPY | list --files counts untracked | `` git-wt list --files `` |  |
| ✅ | HAPPY | bare --files means list | `` git-wt list --files `` |  |
| ✅ | HAPPY | bare -f short flag means list | `` git-wt list -f `` |  |
| ✅ | HAPPY | --files combines with --col | `` git-wt list --col 2 --files `` |  |
| ✅ | HAPPY | list --files honors .gitignore | `` git-wt list --files `` |  |
| ✅ | HAPPY | list without --files has no block | `` git-wt list `` |  |
| ✅ | HAPPY | meld 2 trees passes both dirs | `` git-wt meld 1,3 `` |  |
| ✅ | HAPPY | meld 3 trees, listed order | `` git-wt meld 3,1,6 `` |  |
| ✅ | UNHAPPY | meld one tree errors | `` git-wt meld 1 `` |  |
| ✅ | UNHAPPY | meld over 3 errors | `` git-wt meld 1,2,3,4 `` |  |
| ✅ | UNHAPPY | meld dup tree errors | `` git-wt meld 1,1 `` |  |
| ✅ | UNHAPPY | meld bad index errors | `` git-wt meld 1,99 `` |  |
| ✅ | UNHAPPY | meld non-numeric list errors | `` git-wt meld 1,x `` |  |
| ✅ | UNHAPPY | meld takes no options | `` git-wt meld 1,2 -z `` |  |
| ✅ | UNHAPPY | meld --diff needs 2 trees | `` git-wt meld 1,2,3 --diff `` |  |
| ✅ | HAPPY | meld --diff ... range works | `` git-wt meld 1,3 --diff ... `` |  |
| ✅ | HAPPY | meld --diff ... omits main | `` git-wt meld 1,3 --diff ... `` |  |
| ✅ | HAPPY | meld --diff 2-way works | `` git-wt meld 1,3 --diff `` |  |
| ✅ | HAPPY | meld --diff 2-way has both | `` git-wt meld 1,3 --diff `` |  |
| ✅ | HAPPY | meld --diff empty diff | `` git-wt meld 1,1 --diff `` |  |
| ✅ | UNHAPPY | meld --3way and --base clash | `` git-wt meld 1,2 --diff --3way --base main `` |  |
| ✅ | HAPPY | meld --left/--right two-way | `` git-wt meld --left 1 --right 3 `` |  |
| ✅ | HAPPY | meld --left/--right/--center 3-way | `` git-wt meld --left 3 --center 1 --right 6 `` |  |
| ✅ | HAPPY | meld --center folds into two-way | `` git-wt meld --left 1 --center 3 `` |  |
| ✅ | UNHAPPY | meld --left alone errors | `` git-wt meld --left 1 `` |  |
| ✅ | UNHAPPY | meld --left + positional errors | `` git-wt meld --left 1 --right 3 1,3 `` |  |
| ✅ | HAPPY | meld --left takes a raw path | `` git-wt meld --left /tmp/git-wt-test.U4TZ... --right 3 `` |  |
| ✅ | UNHAPPY | bare target list rejected | `` git-wt 1,2 `` |  |
| ✅ | UNHAPPY | remove rejects a list | `` git-wt remove 1,2 `` |  |
| ✅ | UNHAPPY | meld missing gives install hint | `` git-wt meld 1,2  # PATH without meld `` |  |
| ✅ | HAPPY | compare -x branch | `` git-wt compare -p onlymain.txt -x main `` |  |
| ✅ | HAPPY | compare -x HEAD~1 | `` git-wt compare -p onlymain.txt -x HEAD~1 `` |  |
| ✅ | HAPPY | compare -x sha | `` git-wt compare -p onlymain.txt -x f9cddaf `` |  |
| ✅ | HAPPY | compare -x tag | `` git-wt compare -p onlymain.txt -x cmptag `` |  |
| ✅ | HAPPY | compare -x remote ref | `` git-wt compare -p onlymain.txt -x origin/remote-only `` |  |
| ✅ | HAPPY | compare --ref long form | `` git-wt compare --path onlymain.txt --ref main `` |  |
| ✅ | HAPPY | compare names the file | `` git-wt compare -p onlymain.txt -x main `` |  |
| ✅ | HAPPY | compare -p takes a file list | `` git-wt compare -p onlymain.txt,cmp2.txt -x main `` |  |
| ✅ | HAPPY | compare file list keeps both | `` git-wt compare -p onlymain.txt,cmp2.txt -x main `` |  |
| ✅ | HAPPY | compare -m pairs local first | `` git-wt compare -p onlymain.txt -x main -M `` |  |
| ✅ | HAPPY | compare -m pairs every file | `` git-wt compare -p onlymain.txt,cmp2.txt -x main -M `` |  |
| ✅ | HAPPY | compare -m names the ref | `` git-wt compare -p onlymain.txt -x main -M `` |  |
| ✅ | HAPPY | compare at HEAD prints nothing | `` git-wt compare -p onlymain.txt -x HEAD `` |  |
| ✅ | UNHAPPY | compare bad ref errors | `` git-wt compare -p onlymain.txt -x nope `` |  |
| ✅ | UNHAPPY | compare bad file errors | `` git-wt compare -p nope.txt -x main `` |  |
| ✅ | UNHAPPY | compare empty list part | `` git-wt compare -p onlymain.txt,,cmp2.txt -x main `` |  |
| ✅ | UNHAPPY | compare needs a ref | `` git-wt compare -p onlymain.txt `` |  |
| ✅ | UNHAPPY | compare needs a file | `` git-wt compare -x main `` |  |
| ✅ | UNHAPPY | compare -x takes exactly one | `` git-wt compare -p onlymain.txt -x main,cmptag `` |  |
| ✅ | UNHAPPY | compare -c is retired | `` git-wt compare -p onlymain.txt -c main `` |  |
| ✅ | UNHAPPY | compare --commit is retired | `` git-wt compare -p onlymain.txt --commit main `` |  |
| ✅ | UNHAPPY | compare rejects pre-verb -b | `` git-wt -b main compare -p onlymain.txt -x main `` |  |
| ✅ | UNHAPPY | compare rejects post-verb -b | `` git-wt compare -p onlymain.txt -x main -b main `` |  |
| ✅ | UNHAPPY | remove main refused | `` git-wt remove 1 -y `` |  |
| ✅ | HAPPY | remove other prints nothing | `` git-wt remove 2 -y `` |  |
| ✅ | HAPPY | remove-from-inside prints main | `` git-wt remove 2 -y  # cwd inside it `` |  |
| ✅ | UNHAPPY | remove dirty refused (no -f) | `` git-wt remove 2 -y `` |  |
| ✅ | HAPPY | remove dirty with -f | `` git-wt remove 2 -y -f `` |  |
| ✅ | UNHAPPY | merge takes one source | `` git-wt merge 6 2 `` |  |
| ✅ | UNHAPPY | merge old target-first order rejected | `` git-wt switch 1 merge 2 `` |  |
| ✅ | UNHAPPY | merge unknown source | `` git-wt merge -s zzz `` |  |
| ✅ | UNHAPPY | merge source out of range | `` git-wt merge 99 `` |  |
| ✅ | UNHAPPY | merge source twice refused | `` git-wt merge 6 -s feat-a `` |  |
| ✅ | UNHAPPY | merge -d takes one target | `` git-wt merge 6 -d 1,2 `` |  |
| ✅ | UNHAPPY | merge rejects a source list | `` git-wt merge 1,6 `` |  |
| ✅ | UNHAPPY | merge self refused | `` git-wt merge 1 -d 1 `` |  |
| ✅ | UNHAPPY | merge self by default target | `` git-wt merge 1 `` |  |
| ✅ | UNHAPPY | merge unknown option | `` git-wt merge 6 --rebase `` |  |
| ✅ | UNHAPPY | merge ours+theirs conflict | `` git-wt merge 6 --ours --theirs `` |  |
| ✅ | UNHAPPY | merge dry-run + --no-ff | `` git-wt merge 6 --dry-run --no-ff `` |  |
| ✅ | UNHAPPY | merge continue takes no source | `` git-wt merge 6 --continue `` |  |
| ✅ | UNHAPPY | merge abort takes no source | `` git-wt merge 6 --abort `` |  |
| ✅ | UNHAPPY | merge continue takes no -s | `` git-wt merge -s feat-a --continue `` |  |
| ✅ | UNHAPPY | merge continue with a side | `` git-wt merge --theirs --continue `` |  |
| ✅ | UNHAPPY | merge continue+abort | `` git-wt merge --continue --abort `` |  |
| ✅ | UNHAPPY | rejection names the flag | `` git-wt merge --abort -m x --squash `` |  |
| ✅ | UNHAPPY | merge continue w/o merge | `` git-wt merge --continue -d 1 `` |  |
| ✅ | UNHAPPY | merge abort w/o merge | `` git-wt merge --abort -d 1 `` |  |
| ✅ | HAPPY | merge with untracked only ok | `` git-wt merge 6 -d 5 `` |  |
| ✅ | UNHAPPY | merge into dirty+untracked refused | `` git-wt merge 6 -d 5 `` |  |
| ✅ | UNHAPPY | merge into dirty refused | `` git-wt merge 6 -d 5 `` |  |
| ✅ | HAPPY | merge into dirty with -F | `` git-wt merge 6 -d 5 -F `` |  |
| ✅ | HAPPY | merge by number | `` git-wt merge 6 -d 1 `` |  |
| ✅ | HAPPY | merge by number moved the files | `` test -f a.txt  # in worktree 1 `` |  |
| ✅ | HAPPY | merge prints no stdout | `` git-wt merge 6 -d 2 `` |  |
| ✅ | HAPPY | merge by branch name | `` git-wt merge -s feat-a -d 3 `` |  |
| ✅ | UNHAPPY | merge --ff-only refuses | `` git-wt merge 2 -d 3 --ff-only `` |  |
| ✅ | UNHAPPY | merge conflict reports files | `` git-wt merge 3 -d 2 `` |  |
| ✅ | UNHAPPY | second merge while stuck | `` git-wt merge -s feat-a -d 2 `` |  |
| ✅ | UNHAPPY | continue with unresolved | `` git-wt merge --continue -d 2 `` |  |
| ✅ | HAPPY | continue after resolve | `` git-wt merge --continue -d 2 `` |  |
| ✅ | HAPPY | abort a conflicted merge | `` git-wt merge --abort -d 3 `` |  |
| ✅ | HAPPY | abort clears MERGE_HEAD | `` git rev-parse MERGE_HEAD  # in w-cb2 `` |  |
| ✅ | HAPPY | dry-run clean merge | `` git-wt merge 6 -d 3 --dry-run `` |  |
| ✅ | UNHAPPY | dry-run reports a conflict | `` git-wt merge 4 -d 3 --dry-run `` |  |
| ✅ | UNHAPPY | dry-run names the file | `` git-wt merge 4 -d 3 --dry-run `` |  |
| ✅ | UNHAPPY | dry-run says it touched none | `` git-wt merge 4 -d 3 --dry-run `` |  |
| ✅ | HAPPY | theirs + --dry-run | `` git-wt merge 6 -d 3 --theirs --dry-run `` |  |
| ✅ | HAPPY | ours + --dry-run | `` git-wt merge 6 -d 3 --ours --dry-run `` |  |
| ✅ | HAPPY | dry-run leaves no merge state | `` git rev-parse MERGE_HEAD  # in w-cb2 `` |  |
| ✅ | HAPPY | review clean merge | `` git-wt review 6 -d 8 `` |  |
| ✅ | HAPPY | review of an empty range | `` git-wt review 7 -d 3 `` |  |
| ✅ | HAPPY | review empty says it once | `` git-wt review 7 -t 3 `` |  |
| ✅ | UNHAPPY | review empty still parses | `` git-wt review 7 -d 3 --bogus `` |  |
| ✅ | UNHAPPY | review empty refuses --all | `` git-wt review 7 -d 3 --all `` |  |
| ✅ | HAPPY | review names both branches | `` git-wt review 6 -d 8 `` |  |
| ✅ | UNHAPPY | review reports a conflict | `` git-wt review 4 -d 3 `` |  |
| ✅ | UNHAPPY | review lists the conflict | `` git-wt review 4 -d 3 `` |  |
| ✅ | UNHAPPY | review says it touched none | `` git-wt review 4 -d 3 `` |  |
| ✅ | HAPPY | review -f is files | `` git-wt review 6 -d 8 -f `` |  |
| ✅ | HAPPY | review takes -n | `` git-wt review 6 -d 8 -n 1 `` |  |
| ✅ | HAPPY | review takes --author | `` git-wt review 6 -d 8 --author t `` |  |
| ✅ | HAPPY | review takes --no-merges | `` git-wt review 6 -d 8 --no-merges `` |  |
| ✅ | UNHAPPY | commits still rejects it | `` git-wt commits 8,6 --no-merges `` |  |
| ✅ | UNHAPPY | review rejects -F | `` git-wt review 6 -d 8 -F `` |  |
| ✅ | UNHAPPY | review rejects --dry-run | `` git-wt review 6 -d 8 --dry-run `` |  |
| ✅ | UNHAPPY | review rejects --ff-only | `` git-wt review 6 -d 8 --ff-only `` |  |
| ✅ | UNHAPPY | merge rejects --review | `` git-wt merge 6 -d 8 --review `` |  |
| ✅ | UNHAPPY | merge rejects --meld | `` git-wt merge 6 -d 8 --meld `` |  |
| ✅ | HAPPY | review + squash consolidates | `` git-wt review 6 -d 8 --squash `` |  |
| ✅ | UNHAPPY | review keeps typo errors | `` git-wt review 6 -d 8 --bogus `` |  |
| ✅ | UNHAPPY | review refuses --all | `` git-wt review 6 -d 8 --all `` |  |
| ✅ | UNHAPPY | review refuses -a as --all | `` git-wt review 6 -d 8 -a `` |  |
| ✅ | UNHAPPY | review refuses --union | `` git-wt review 6 -d 8 --union `` |  |
| ✅ | UNHAPPY | review needs a source | `` git-wt review -d 8 `` |  |
| ✅ | UNHAPPY | review source twice refused | `` git-wt review 6 -s feat-a `` |  |
| ✅ | UNHAPPY | review -d takes one target | `` git-wt review 6 -d 1,2 `` |  |
| ✅ | UNHAPPY | review rejects a source list | `` git-wt review 1,6 `` |  |
| ✅ | UNHAPPY | review self refused | `` git-wt review 1 -d 1 `` |  |
| ✅ | HAPPY | review -s names the source | `` git-wt review -s feat-a -d 8 `` |  |
| ✅ | HAPPY | review leaves no merge state | `` git rev-parse MERGE_HEAD  # in w-cb2 `` |  |
| ✅ | HAPPY | merged current in itself | `` git-wt merged 1 `` |  |
| ✅ | UNHAPPY | merged branch not in main | `` git-wt merged 1 cb3 `` |  |
| ✅ | HAPPY | merged branch is in cb2 | `` git-wt merged 3 stuckbr `` |  |
| ✅ | UNHAPPY | merged list form dest-first | `` git-wt merged 1,4 `` |  |
| ✅ | HAPPY | merged list form reversed | `` git-wt merged 3,7 `` |  |
| ✅ | UNHAPPY | merged too many args | `` git-wt merged 1 cb3 extra `` |  |
| ✅ | UNHAPPY | merged unknown source | `` git-wt merged 1 zzz `` |  |
| ✅ | UNHAPPY | merged self single form | `` git-wt merged 1 1 `` |  |
| ✅ | UNHAPPY | merged single target with number source | `` git-wt merged 1 4 `` |  |
| ✅ | UNHAPPY | merged list too many | `` git-wt merged 1,4,3 `` |  |
| ✅ | UNHAPPY | merged list form extra arg | `` git-wt merged 1,4 extra `` |  |
| ✅ | UNHAPPY | merged list form dup | `` git-wt merged 1,1 `` |  |
| ✅ | HAPPY | merged N self-check | `` git-wt merged 2 `` |  |
| ✅ | HAPPY | merged detached list form | `` git-wt merged 1,5 `` |  |
| ✅ | UNHAPPY | merged detached number source needs branch | `` git-wt merged 1 5 `` |  |
| ✅ | HAPPY | list --col 6 | `` git-wt list --col 1,2,6 `` |  |
| ✅ | HAPPY | merge theirs resolves | `` git-wt merge 4 -d 3 --theirs `` |  |
| ✅ | HAPPY | theirs took the source's side | `` cat shared.txt  # in w-cb2 `` |  |
| ✅ | HAPPY | merge ours keeps our side | `` git-wt merge -s cb4 -d 2 --ours `` |  |
| ✅ | HAPPY | ours kept worktree N's side | `` cat shared.txt  # in w-cb1 `` |  |
| ✅ | HAPPY | stuck+theirs declined | `` git-wt merge 4 -d 7 --theirs `` |  |
| ✅ | UNHAPPY | declining keeps the stopped merge | `` git rev-parse MERGE_HEAD  # in w-ff2 `` |  |
| ✅ | HAPPY | stuck+theirs accepted | `` git-wt merge 4 -d 7 --theirs `` |  |
| ✅ | HAPPY | redo let theirs win | `` cat shared.txt  # in w-ff2 `` |  |
| ✅ | HAPPY | explicit -d dry-run clean | `` git-wt merge 6 -d 1 --dry-run `` |  |
| ✅ | UNHAPPY | explicit -d takes options | `` git-wt merge 4 -d 2 --dry-run `` |  |
| ✅ | UNHAPPY | -d rejects 3 targets | `` git-wt merge 6 -d 1,2,4 `` |  |
| ✅ | UNHAPPY | bare list without verb rejected | `` git-wt 1,6 `` |  |
| ✅ | UNHAPPY | malformed list with verb rejected | `` git-wt 1, merge `` |  |
| ✅ | UNHAPPY | list form + verb order rejected | `` git-wt 1,6 merge continue `` |  |
| ✅ | UNHAPPY | list form + short flag order rejected | `` git-wt 1,6 merge -a `` |  |
| ✅ | UNHAPPY | bad list + verb order rejected | `` git-wt 1,x merge `` |  |
| ✅ | HAPPY | -d merges the source into it | `` git-wt merge 6 -d 8 `` |  |
| ✅ | HAPPY | -d moved the files | `` test -f a.txt  # in w-lm `` |  |
| ✅ | HAPPY | merge --squash stages only | `` git-wt merge 6 -d 7 --squash `` |  |
| ✅ | HAPPY | --squash leaves changes staged | `` git diff --cached  # in w-ff `` |  |
| ✅ | HAPPY | sync fetch bare verb defaults to current | `` git-wt fetch `` |  |
| ✅ | UNHAPPY | sync target + --all | `` git-wt pull 1 --all `` |  |
| ✅ | UNHAPPY | sync list + --all | `` git-wt push 1,3 --all `` |  |
| ✅ | UNHAPPY | sync list dup | `` git-wt fetch 1,1 `` |  |
| ✅ | UNHAPPY | sync unknown flag | `` git-wt pull 1 --depth=1 `` |  |
| ✅ | UNHAPPY | sync flags are per verb | `` git-wt fetch 1 --rebase `` |  |
| ✅ | UNHAPPY | sync push has no --rebase | `` git-wt push 1 --rebase `` |  |
| ✅ | UNHAPPY | sync pull has no -u | `` git-wt pull 1 -u `` |  |
| ✅ | UNHAPPY | sync push --force refused | `` git-wt push 1 --force `` |  |
| ✅ | UNHAPPY | sync push -F refused | `` git-wt push 1 -F `` |  |
| ✅ | UNHAPPY | sync contradiction | `` git-wt pull 1 --rebase --no-rebase `` |  |
| ✅ | UNHAPPY | sync rebase vs ff-only | `` git-wt pull 1 --rebase --ff-only `` |  |
| ✅ | HAPPY | fetch one worktree | `` git-wt fetch 1 `` |  |
| ✅ | HAPPY | fetch takes --prune | `` git-wt fetch 1 --prune `` |  |
| ✅ | HAPPY | fetch list form | `` git-wt fetch 1,3 `` |  |
| ✅ | HAPPY | fetch --all sweeps | `` git-wt fetch --all `` |  |
| ✅ | HAPPY | fetch detached is not skipped | `` git-wt fetch 1,2 `` |  |
| ✅ | HAPPY | pull one worktree | `` git-wt pull 1 `` |  |
| ✅ | HAPPY | sync bare verb defaults to current | `` git-wt pull `` |  |
| ✅ | HAPPY | pull moved the branch | `` git log -1  # in syn/app `` |  |
| ✅ | UNHAPPY | pull --all keeps going | `` git-wt pull --all `` |  |
| ✅ | UNHAPPY | pull --all names the failure | `` git-wt pull --all `` |  |
| ✅ | UNHAPPY | pull --all skips detached | `` git-wt pull --all `` |  |
| ✅ | UNHAPPY | pull single error is git's | `` git-wt pull 4 `` |  |
| ✅ | HAPPY | pull takes --ff-only | `` git-wt pull 1 --ff-only `` |  |
| ✅ | HAPPY | push -u sets the upstream | `` git-wt push 4 -u `` |  |
| ✅ | HAPPY | push -u left an upstream | `` git rev-parse --abbrev-ref @{u}  # in w-lonely `` |  |
| ✅ | HAPPY | push -u again is fine | `` git-wt push 4 -u `` |  |
| ✅ | HAPPY | push --all sweeps | `` git-wt push --all `` |  |
| ✅ | HAPPY | push takes --dry-run | `` git-wt push 1 --dry-run `` |  |
| ✅ | HAPPY | version | `` git-wt version `` |  |
| ✅ | HAPPY | --help | `` git-wt --help `` |  |

## Summary

- Passed: **425**
- Failed: **0**
- Total: **425**
