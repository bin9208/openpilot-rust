"""Deterministic complete histories for the original monitoring policy oracle."""
from copy import deepcopy
import random


def driver():
    return {'face_orientation': [0., 0., 0.], 'face_position': [0., 0.], 'face_orientation_std': [0., 0., 0.],
                'face_position_std': [0., 0.], 'face_prob': 1., 'left_eye_prob': 1., 'right_eye_prob': 1.,
                'left_blink_prob': 0., 'right_blink_prob': 0., 'sunglasses_prob': 0., 'phone_prob': 0., 'sleep_prob': 0.}


def template():
    return {'driver': {'left': driver(), 'right': driver(), 'wheel_on_right_prob': 0.}, 'car_speed': 20., 'enabled': True,
                'wrong_gear': False, 'steering_pressed': False, 'gas_pressed': False, 'brake_disengage_prob': 1.,
                'steering_angle_deg': 0., 'calibration': [0., 0., 0.], 'demo': False}


def fixtures():
    scenarios = {'attentive-calibration': 8000, 'sleep-red-lockout-recovery': 37000, 'two-red-lockout': 650,
                 'missing-face': 800, 'uncertain-fallback-reset': 1700, 'orange-hide-recover': 900,
                 'wheel-touch-recover': 900, 'low-speed-stop-launch': 1000, 'always-on-gear': 1200,
                 'demo': 500, 'wheel-switch': 1100, 'empty-arrays': 120, 'thresholds': 1000,
                 'saved-lockout': 36002, 'calibration-offset-limits': 1700, 'randomized': 12000}
    for rhd in (False, True):
        for scenario, count in scenarios.items():
            rng = random.Random(20260930 + int(rhd))
            for frame in range(count):
                value = template()
                value['driver']['wheel_on_right_prob'] = float(rhd)
                sides = [value['driver']['left'], value['driver']['right']]
                if scenario in ('sleep-red-lockout-recovery', 'two-red-lockout'):
                    for side in sides:
                        side['sleep_prob'] = .9
                    if scenario == 'sleep-red-lockout-recovery' and frame >= 400:
                        value['enabled'] = False
                    if scenario == 'two-red-lockout':
                        value['enabled'] = frame != 280
                elif scenario == 'missing-face':
                    for side in sides:
                        side['face_prob'] = 0.
                elif scenario == 'uncertain-fallback-reset':
                    for side in sides:
                        side['face_orientation_std'] = [0.4 if frame < 1300 else 0.] * 3
                    value['gas_pressed'] = frame == 900
                elif scenario == 'orange-hide-recover':
                    for side in sides:
                        side['phone_prob'] = float(frame < 400)
                        side['face_prob'] = float(not 180 <= frame < 350)
                    value['steering_pressed'] = 200 <= frame < 250 or frame == 420
                elif scenario == 'wheel-touch-recover':
                    for side in sides:
                        side['face_prob'] = float(400 <= frame < 410 or frame >= 800)
                    value['steering_pressed'] = frame == 320
                elif scenario in ('low-speed-stop-launch', 'always-on-gear', 'demo'):
                    for side in sides:
                        side['left_blink_prob'] = side['right_blink_prob'] = 1.
                    if scenario == 'low-speed-stop-launch':
                        value['car_speed'] = 0. if 200 <= frame < 600 else 20.
                    else:
                        value['enabled'] = False
                        value['wrong_gear'] = frame >= 1000
                        value['demo'] = scenario == 'demo'
                        value['car_speed'] = 0. if value['demo'] else 20.
                elif scenario == 'wheel-switch':
                    value['driver']['wheel_on_right_prob'] = float(not rhd)
                    value['enabled'] = frame < 700
                    sides[int(not rhd)]['phone_prob'] = 1.
                elif scenario == 'empty-arrays':
                    if frame > 20:
                        for side in sides:
                            side[['face_orientation', 'face_position', 'face_orientation_std', 'face_position_std'][frame % 4]] = None
                elif scenario == 'calibration-offset-limits':
                    value['enabled'] = frame >= 1400
                    for side in sides:
                        side['face_orientation'] = [.3 if rhd else -.3, -.5 if rhd else .5, 0.]
                    value['steering_angle_deg'] = [-270., 0., 270.][frame % 3]
                elif scenario == 'randomized':
                    value.update(car_speed=rng.uniform(0, 40), enabled=rng.random() < .9,
                                 wrong_gear=rng.random() < .1, steering_pressed=rng.random() < .1,
                                 gas_pressed=rng.random() < .1, brake_disengage_prob=rng.uniform(-.1, 1.1),
                                 steering_angle_deg=rng.uniform(-360, 360),
                                 calibration=[0., rng.uniform(-.1, .1), rng.uniform(-.1, .1)])
                    value['driver']['wheel_on_right_prob'] = rng.random()
                    for side in sides:
                        side.update(face_orientation=[rng.uniform(-.6, .6), rng.uniform(-.6, .6), 0.],
                                    face_position=[rng.uniform(-.3, .3), rng.uniform(-.3, .3)],
                                    face_orientation_std=[rng.choice([0., .1, .3, .4])] * 3,
                                    face_prob=rng.choice([0., .7, .8, 1.]), left_eye_prob=rng.random(),
                                    right_eye_prob=rng.random(), sunglasses_prob=rng.random(),
                                    left_blink_prob=rng.random(), right_blink_prob=rng.random(),
                                    phone_prob=rng.random(), sleep_prob=rng.random())
                elif scenario == 'thresholds':
                    value['brake_disengage_prob'] = [-.1, 0., .1, .5, 1.][frame % 5]
                    value['car_speed'] = [2.79, 2.8, 11., 11.01, 13., 13.01, 30.][frame % 7]
                    value['steering_angle_deg'] = [-270., -30., 0., 30., 270.][frame % 5]
                    value['calibration'] = [0., .03, -.04]
                    for side in sides:
                        side.update(face_orientation=[[-.5, .3, 0.][frame % 3], [-.5, 0., .5][frame % 3], 0.],
                                    face_position=[-.1 + (frame % 5) * .05, -.05 + (frame % 3) * .05],
                                    face_prob=[.69, .7, .71, 1.][frame % 4], left_eye_prob=[.65, .66][frame % 2],
                                    right_eye_prob=[.65, .66][frame % 2], sunglasses_prob=[.89, .9][frame % 2],
                                    left_blink_prob=[.864, .865, .866][frame % 3], right_blink_prob=[.864, .865, .866][frame % 3],
                                    phone_prob=[.49, .5, .51][frame % 3], sleep_prob=[.74, .75, .76][frame % 3])
                yield {'reset': frame == 0, 'rhd_saved': rhd, 'always_on': scenario == 'always-on-gear',
                           'too_distracted': scenario == 'saved-lockout', 'valid': frame % 17 != 0,
                           'input': deepcopy(value), 'scenario': f'{scenario}-rhd{int(rhd)}', 'frame': frame}
