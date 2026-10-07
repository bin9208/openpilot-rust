from typing import Final

EXTENDED_BRANDS: Final = {
  'MAZDA_CX5_2022': 'mazda', 'NISSAN_XTRAIL': 'nissan', 'NISSAN_LEAF': 'nissan', 'NISSAN_ALTIMA': 'nissan',
  'CHRYSLER_PACIFICA_2018': 'chrysler', 'RAM_1500_5TH_GEN': 'chrysler', 'RAM_HD_5TH_GEN': 'chrysler', 'RIVIAN_R1_GEN1': 'rivian',
  'FORD_F_150_MK14': 'ford', 'FORD_MAVERICK_MK1': 'ford',
  'SUBARU_ASCENT': 'subaru', 'SUBARU_OUTBACK_2023': 'subaru', 'SUBARU_FORESTER_PREGLOBAL': 'subaru',
  'TOYOTA_PRIUS': 'toyota', 'TOYOTA_RAV4_TSS2': 'toyota', 'TOYOTA_RAV4_PRIME': 'toyota',
  'CHEVROLET_VOLT': 'gm', 'CHEVROLET_BOLT_EUV': 'gm',
  'HONDA_CIVIC': 'honda', 'HONDA_ACCORD': 'honda', 'HONDA_CIVIC_2022': 'honda', 'HONDA_CRV_5G': 'honda',
  'VOLKSWAGEN_PASSAT_NMS': 'volkswagen', 'VOLKSWAGEN_ID4_MK1': 'volkswagen', 'VOLKSWAGEN_ID4_MK2': 'volkswagen',
}
CORPUS_DIRECTORIES: Final = {
  'ford': 'sealed-green', 'subaru': 'sealed', 'toyota': 'runtime-green', 'gm': 'final', 'honda': 'runtime-green', 'volkswagen': 'verified-runtime',
}
FIXTURE_NAMES: Final = {
  'HONDA_CRV_5G': 'runtime-HONDA_CRV_5G-True',
  **{candidate: f'runtime-{candidate}-True-False-0' for candidate, brand in EXTENDED_BRANDS.items() if brand == 'volkswagen'},
}
