# #535 나머지 native ARM 후보5개

2026-10-25. 새 두 workflow는 webcam/Xiaoge/radard와 RadarCAN/Carrot Navi ARM의 기존 기능을 기본ARM VM에서 관측한다. 기존 main의5개 ARM 범위를 아직 제거하지 않는다. 새 기능/빌드/플랫폼/봉인 결과는 Actions UNRUN이다.

각각90/75/90/90/60분이며 원래 기능run·source/index·최종upload/cleanup·원본비교·수치기준을 유지한다. QEMU 준비/마감만 수용된native adapter로 바꾸고 실제Python3.12.14 privatevenv를 준비한다. Carrot Navi의venv 준비는 원래guest가공급하던계약을충족하며512MiB 순차성장admission을별도로확인한다. 60GBfit/전체SDK·DSOclosure/실기수용을소급주장하지않는다.

모두정확matrix.include=[{arch:aarch64,runner:hosted-arm64}]단일행이다. radard의두uname/readelfbody에서matrix.machine을matrix.arch로치환했고실제aarch64대입후명령bytes는원문ARM행과같다. 다른기능기준은바꾸지않는다.

helper는정확한두caller경로와같은codexbranchpush만추가허용한다. maincaller·기존candidate경계는그대로다. source-layout정책의논리job5개와ARM5개를추가해66/14→71/19를검사하며기존테스트를줄이지않는다. 새CI의원문Python/venv호출은각각execution-edge pending5개로등록하며 baseline3580 credit0이다.

초기source검토와행렬P2보완뒤YAML/Bash문법을확인했다. 실제provider·apt/wheel·성장량·기능·upload/cleanup은새정확소스Actions에서만판정한다. 먼저모든후보의실제결과를확인하고필수main ARM경로이동은후속으로진행한다.
