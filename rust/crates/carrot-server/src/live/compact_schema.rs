//! Display schema ported from carrot/realtime/compact_state.py; original source license applies.
use super::compact_fields::{Field, Spec};
pub(super) const SERVICES: &[(&str, u8, &[Field])] = &[
    (
        "carState",
        1,
        &[
            Field {
                path: &["vEgo"],
                spec: Spec::F32,
            },
            Field {
                path: &["aEgo"],
                spec: Spec::F32,
            },
            Field {
                path: &["vEgoCluster"],
                spec: Spec::F32,
            },
            Field {
                path: &["vCruiseCluster"],
                spec: Spec::F32,
            },
            Field {
                path: &["steeringAngleDeg"],
                spec: Spec::F32,
            },
            Field {
                path: &["brakeHoldActive"],
                spec: Spec::Bool,
            },
            Field {
                path: &["softHoldActive"],
                spec: Spec::I16,
            },
            Field {
                path: &["carrotCruise"],
                spec: Spec::I16,
            },
            Field {
                path: &["gearStep"],
                spec: Spec::I16,
            },
            Field {
                path: &["useLaneLineSpeed"],
                spec: Spec::F32,
            },
            Field {
                path: &["brakeLights"],
                spec: Spec::Bool,
            },
            Field {
                path: &["leftBlindspot"],
                spec: Spec::Bool,
            },
            Field {
                path: &["rightBlindspot"],
                spec: Spec::Bool,
            },
            Field {
                path: &["leftLaneLine"],
                spec: Spec::I16,
            },
            Field {
                path: &["rightLaneLine"],
                spec: Spec::I16,
            },
            Field {
                path: &["gearShifter"],
                spec: Spec::Enum(&[
                    "unknown",
                    "park",
                    "drive",
                    "neutral",
                    "reverse",
                    "sport",
                    "low",
                    "brake",
                    "eco",
                    "manumatic",
                ]),
            },
            Field {
                path: &["leftBlinker"],
                spec: Spec::Bool,
            },
            Field {
                path: &["rightBlinker"],
                spec: Spec::Bool,
            },
            Field {
                path: &["fuelGauge"],
                spec: Spec::F32,
            },
            Field {
                path: &["ureaGauge"],
                spec: Spec::F32,
            },
            Field {
                path: &["tpms"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["fl"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fr"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["rl"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["rr"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["evModeValid"],
                spec: Spec::Bool,
            },
            Field {
                path: &["evModeActive"],
                spec: Spec::Bool,
            },
        ],
    ),
    (
        "controlsState",
        2,
        &[
            Field {
                path: &["deprecated", "enabled"],
                spec: Spec::Bool,
            },
            Field {
                path: &["deprecated", "vCruiseCluster"],
                spec: Spec::F32,
            },
            Field {
                path: &["activeLaneLine"],
                spec: Spec::Bool,
            },
            Field {
                path: &["curvature"],
                spec: Spec::F32,
            },
            Field {
                path: &["desiredCurvature"],
                spec: Spec::F32,
            },
            Field {
                path: &["lateralControlState", "torqueState", "actualLateralAccel"],
                spec: Spec::F32,
            },
            Field {
                path: &["lateralControlState", "torqueState", "desiredLateralAccel"],
                spec: Spec::F32,
            },
            Field {
                path: &["lateralControlState", "torqueState", "output"],
                spec: Spec::F32,
            },
        ],
    ),
    (
        "deviceState",
        3,
        &[
            Field {
                path: &["memoryUsagePercent"],
                spec: Spec::I8,
            },
            Field {
                path: &["freeSpacePercent"],
                spec: Spec::F32,
            },
            Field {
                path: &["cpuTempC"],
                spec: Spec::F32List,
            },
            Field {
                path: &["deviceType"],
                spec: Spec::Enum(&[
                    "unknown",
                    "neo",
                    "chffrAndroid",
                    "chffrIos",
                    "tici",
                    "pc",
                    "tizi",
                    "mici",
                ]),
            },
            Field {
                path: &["started"],
                spec: Spec::Bool,
            },
        ],
    ),
    (
        "peripheralState",
        4,
        &[Field {
            path: &["voltage"],
            spec: Spec::U32,
        }],
    ),
    (
        "carrotMan",
        5,
        &[
            Field {
                path: &["activeCarrot"],
                spec: Spec::I32,
            },
            Field {
                path: &["nRoadLimitSpeed"],
                spec: Spec::I32,
            },
            Field {
                path: &["xSpdType"],
                spec: Spec::I32,
            },
            Field {
                path: &["xSpdLimit"],
                spec: Spec::I32,
            },
            Field {
                path: &["xSpdDist"],
                spec: Spec::I32,
            },
            Field {
                path: &["xSpdCountDown"],
                spec: Spec::I32,
            },
            Field {
                path: &["xTurnInfo"],
                spec: Spec::I32,
            },
            Field {
                path: &["xDistToTurn"],
                spec: Spec::I32,
            },
            Field {
                path: &["xTurnCountDown"],
                spec: Spec::I32,
            },
            Field {
                path: &["atcType"],
                spec: Spec::Text,
            },
            Field {
                path: &["szPosRoadName"],
                spec: Spec::Text,
            },
            Field {
                path: &["szTBTMainText"],
                spec: Spec::Text,
            },
            Field {
                path: &["desiredSpeed"],
                spec: Spec::I32,
            },
            Field {
                path: &["xPosLat"],
                spec: Spec::F32,
            },
            Field {
                path: &["xPosLon"],
                spec: Spec::F32,
            },
            Field {
                path: &["xPosAngle"],
                spec: Spec::F32,
            },
            Field {
                path: &["xPosSpeed"],
                spec: Spec::F32,
            },
            Field {
                path: &["trafficState"],
                spec: Spec::I32,
            },
            Field {
                path: &["nGoPosDist"],
                spec: Spec::I32,
            },
            Field {
                path: &["nGoPosTime"],
                spec: Spec::I32,
            },
            Field {
                path: &["szSdiDescr"],
                spec: Spec::Text,
            },
            Field {
                path: &["naviPaths"],
                spec: Spec::Text,
            },
            Field {
                path: &["desiredSource"],
                spec: Spec::Text,
            },
            Field {
                path: &["vehicleNaviActive"],
                spec: Spec::Bool,
            },
            Field {
                path: &["vehicleNaviSpeed"],
                spec: Spec::I32,
            },
            Field {
                path: &["vehicleNaviSectionActive"],
                spec: Spec::Bool,
            },
            Field {
                path: &["vehicleNaviAvailable"],
                spec: Spec::Bool,
            },
            Field {
                path: &["naviOwner"],
                spec: Spec::Text,
            },
            Field {
                path: &["naviSessionId"],
                spec: Spec::Text,
            },
            Field {
                path: &["naviSequence"],
                spec: Spec::U64,
            },
            Field {
                path: &["naviOwnerAgeMs"],
                spec: Spec::I32,
            },
            Field {
                path: &["naviSafetyAgeMs"],
                spec: Spec::I32,
            },
            Field {
                path: &["naviLifecycle"],
                spec: Spec::Text,
            },
            Field {
                path: &["naviControlAllowed"],
                spec: Spec::Bool,
            },
            Field {
                path: &["naviSafetyRejection"],
                spec: Spec::Text,
            },
            Field {
                path: &["decelProvider"],
                spec: Spec::Text,
            },
            Field {
                path: &["decelReason"],
                spec: Spec::Text,
            },
        ],
    ),
    (
        "selfdriveState",
        6,
        &[
            Field {
                path: &["enabled"],
                spec: Spec::Bool,
            },
            Field {
                path: &["personality"],
                spec: Spec::Enum(&["aggressive", "standard", "relaxed", "moreRelaxed"]),
            },
            Field {
                path: &["alertStatus"],
                spec: Spec::Enum(&["normal", "userPrompt", "critical"]),
            },
            Field {
                path: &["alertSize"],
                spec: Spec::Enum(&["none", "small", "mid", "full"]),
            },
            Field {
                path: &["alertType"],
                spec: Spec::Text,
            },
            Field {
                path: &["alertText1"],
                spec: Spec::Text,
            },
            Field {
                path: &["alertText2"],
                spec: Spec::Text,
            },
        ],
    ),
    (
        "gpsLocationExternal",
        7,
        &[
            Field {
                path: &["latitude"],
                spec: Spec::F64,
            },
            Field {
                path: &["longitude"],
                spec: Spec::F64,
            },
            Field {
                path: &["speed"],
                spec: Spec::F32,
            },
            Field {
                path: &["bearingDeg"],
                spec: Spec::F32,
            },
            Field {
                path: &["bearingAccuracyDeg"],
                spec: Spec::F32,
            },
            Field {
                path: &["speedAccuracy"],
                spec: Spec::F32,
            },
            Field {
                path: &["hasFix"],
                spec: Spec::Bool,
            },
            Field {
                path: &["altitude"],
                spec: Spec::F64,
            },
            Field {
                path: &["horizontalAccuracy"],
                spec: Spec::F32,
            },
            Field {
                path: &["verticalAccuracy"],
                spec: Spec::F32,
            },
            Field {
                path: &["unixTimestampMillis"],
                spec: Spec::F64,
            },
        ],
    ),
    (
        "longitudinalPlan",
        8,
        &[
            Field {
                path: &["accels"],
                spec: Spec::F32FirstList,
            },
            Field {
                path: &["speeds"],
                spec: Spec::F32FirstList,
            },
            Field {
                path: &["jerks"],
                spec: Spec::F32FirstList,
            },
            Field {
                path: &["tFollow"],
                spec: Spec::F32,
            },
            Field {
                path: &["desiredDistance"],
                spec: Spec::F32,
            },
            Field {
                path: &["myDrivingMode"],
                spec: Spec::I32,
            },
            Field {
                path: &["xState"],
                spec: Spec::I32,
            },
            Field {
                path: &["trafficState"],
                spec: Spec::I32,
            },
            Field {
                path: &["longitudinalPlanSource"],
                spec: Spec::Enum(&["cruise", "lead0", "lead1", "lead2", "e2e"]),
            },
            Field {
                path: &["cruiseTarget"],
                spec: Spec::F32,
            },
        ],
    ),
    (
        "modelV2",
        9,
        &[
            Field {
                path: &["frameId"],
                spec: Spec::U32,
            },
            Field {
                path: &["frameIdExtra"],
                spec: Spec::U32,
            },
            Field {
                path: &["position"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["x"],
                        spec: Spec::U16CmList,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::I16MmList,
                    },
                    Field {
                        path: &["z"],
                        spec: Spec::I16MmList,
                    },
                ]),
            },
            Field {
                path: &["velocity"],
                spec: Spec::Struct(&[Field {
                    path: &["x"],
                    spec: Spec::I16CmList,
                }]),
            },
            Field {
                path: &["laneLines"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["x"],
                        spec: Spec::U16CmList,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::I16MmList,
                    },
                    Field {
                        path: &["z"],
                        spec: Spec::I16MmList,
                    },
                ]),
            },
            Field {
                path: &["laneLineProbs"],
                spec: Spec::F32List,
            },
            Field {
                path: &["roadEdges"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["x"],
                        spec: Spec::U16CmList,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::I16MmList,
                    },
                    Field {
                        path: &["z"],
                        spec: Spec::I16MmList,
                    },
                ]),
            },
            Field {
                path: &["roadEdgeStds"],
                spec: Spec::F32List,
            },
            Field {
                path: &["leadsV3"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["prob"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["x"],
                        spec: Spec::U16CmList,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::F32FirstList,
                    },
                    Field {
                        path: &["v"],
                        spec: Spec::F32FirstList,
                    },
                ]),
            },
            Field {
                path: &["laneLineStds"],
                spec: Spec::F32List,
            },
            Field {
                path: &["frameAge"],
                spec: Spec::I32,
            },
            Field {
                path: &["frameDropPerc"],
                spec: Spec::F32,
            },
            Field {
                path: &["modelExecutionTime"],
                spec: Spec::F32,
            },
        ],
    ),
    (
        "liveCalibration",
        10,
        &[
            Field {
                path: &["calStatus"],
                spec: Spec::Enum(&["uncalibrated", "calibrated", "invalid", "recalibrating"]),
            },
            Field {
                path: &["calCycle"],
                spec: Spec::I32,
            },
            Field {
                path: &["calPerc"],
                spec: Spec::I8,
            },
            Field {
                path: &["validBlocks"],
                spec: Spec::I32,
            },
            Field {
                path: &["rpyCalib"],
                spec: Spec::F32List,
            },
            Field {
                path: &["height"],
                spec: Spec::F32List,
            },
        ],
    ),
    (
        "roadCameraState",
        11,
        &[
            Field {
                path: &["frameId"],
                spec: Spec::U32,
            },
            Field {
                path: &["sensor"],
                spec: Spec::Enum(&["unknown", "ar0231", "ox03c10", "os04c10"]),
            },
            Field {
                path: &["timestampEof"],
                spec: Spec::U64,
            },
        ],
    ),
    (
        "lateralPlan",
        12,
        &[
            Field {
                path: &["useLaneLines"],
                spec: Spec::Bool,
            },
            Field {
                path: &["latDebugText"],
                spec: Spec::Text,
            },
            Field {
                path: &["position"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["x"],
                        spec: Spec::U16CmList,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::I16MmList,
                    },
                    Field {
                        path: &["z"],
                        spec: Spec::I16MmList,
                    },
                ]),
            },
            Field {
                path: &["distances"],
                spec: Spec::F32List,
            },
            Field {
                path: &["laneChangeState"],
                spec: Spec::Enum(&[
                    "off",
                    "preLaneChange",
                    "laneChangeStarting",
                    "laneChangeFinishing",
                ]),
            },
            Field {
                path: &["laneChangeDirection"],
                spec: Spec::Enum(&["none", "left", "right"]),
            },
        ],
    ),
    (
        "radarState",
        13,
        &[
            Field {
                path: &["leadOne"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadTwo"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadRight"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadLeft"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadsLeft"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadsCenter"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadsRight"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadsLeft2"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadsRight2"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
            Field {
                path: &["leadsCutIn"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["dRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aRel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["dPath"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLat"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["vLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["aLeadK"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["fcw"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["status"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["aLeadTau"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["modelProb"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["radar"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["radarTrackId"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["jLead"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["score"],
                        spec: Spec::F32,
                    },
                ]),
            },
        ],
    ),
    (
        "carControl",
        14,
        &[
            Field {
                path: &["latActive"],
                spec: Spec::Bool,
            },
            Field {
                path: &["longActive"],
                spec: Spec::Bool,
            },
            Field {
                path: &["actuators"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["steeringAngleDeg"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["accel"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["curvature"],
                        spec: Spec::F32,
                    },
                ]),
            },
        ],
    ),
    (
        "liveDelay",
        15,
        &[
            Field {
                path: &["lateralDelay"],
                spec: Spec::F32,
            },
            Field {
                path: &["calPerc"],
                spec: Spec::I8,
            },
        ],
    ),
    (
        "liveTorqueParameters",
        16,
        &[
            Field {
                path: &["liveValid"],
                spec: Spec::Bool,
            },
            Field {
                path: &["latAccelFactorFiltered"],
                spec: Spec::F32,
            },
            Field {
                path: &["frictionCoefficientFiltered"],
                spec: Spec::F32,
            },
            Field {
                path: &["calPerc"],
                spec: Spec::I8,
            },
        ],
    ),
    (
        "liveParameters",
        17,
        &[
            Field {
                path: &["angleOffsetDeg"],
                spec: Spec::F32,
            },
            Field {
                path: &["steerRatio"],
                spec: Spec::F32,
            },
        ],
    ),
    (
        "liveTracks",
        18,
        &[Field {
            path: &["points"],
            spec: Spec::StructList(&[
                Field {
                    path: &["trackId"],
                    spec: Spec::U32,
                },
                Field {
                    path: &["dRel"],
                    spec: Spec::F32,
                },
                Field {
                    path: &["yRel"],
                    spec: Spec::F32,
                },
                Field {
                    path: &["vRel"],
                    spec: Spec::F32,
                },
                Field {
                    path: &["measured"],
                    spec: Spec::Bool,
                },
                Field {
                    path: &["radarSource"],
                    spec: Spec::Enum(&["frontRadar", "scc", "corner235", "corner180", "corner430"]),
                },
            ]),
        }],
    ),
    (
        "cameraOdometry",
        19,
        &[
            Field {
                path: &["frameId"],
                spec: Spec::U32,
            },
            Field {
                path: &["timestampEof"],
                spec: Spec::U64,
            },
            Field {
                path: &["trans"],
                spec: Spec::F32List,
            },
            Field {
                path: &["rot"],
                spec: Spec::F32List,
            },
            Field {
                path: &["transStd"],
                spec: Spec::F32List,
            },
            Field {
                path: &["rotStd"],
                spec: Spec::F32List,
            },
        ],
    ),
    (
        "carrotNavi",
        21,
        &[
            Field {
                path: &["schemaVersion"],
                spec: Spec::U16,
            },
            Field {
                path: &["generation"],
                spec: Spec::U64,
            },
            Field {
                path: &["sessionId"],
                spec: Spec::Text,
            },
            Field {
                path: &["publishMonoTimeNanos"],
                spec: Spec::U64,
            },
            Field {
                path: &["connected"],
                spec: Spec::Bool,
            },
            Field {
                path: &["vehicle"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["meta", "present"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["latitude"],
                        spec: Spec::F64,
                    },
                    Field {
                        path: &["longitude"],
                        spec: Spec::F64,
                    },
                    Field {
                        path: &["headingDeg"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["speedKph"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["roadName"],
                        spec: Spec::Text,
                    },
                ]),
            },
            Field {
                path: &["guidanceCurrent"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["meta", "present"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["distanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["timeSec"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["turnType"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["roadName"],
                        spec: Spec::Text,
                    },
                    Field {
                        path: &["mainText"],
                        spec: Spec::Text,
                    },
                    Field {
                        path: &["pointValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["latitude"],
                        spec: Spec::F64,
                    },
                    Field {
                        path: &["longitude"],
                        spec: Spec::F64,
                    },
                ]),
            },
            Field {
                path: &["guidanceNext"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["meta", "present"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["distanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["timeSec"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["turnType"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["roadName"],
                        spec: Spec::Text,
                    },
                    Field {
                        path: &["mainText"],
                        spec: Spec::Text,
                    },
                    Field {
                        path: &["pointValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["latitude"],
                        spec: Spec::F64,
                    },
                    Field {
                        path: &["longitude"],
                        spec: Spec::F64,
                    },
                ]),
            },
            Field {
                path: &["laneCurrent"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["meta", "present"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["count"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["distanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["visible"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["available"],
                        spec: Spec::I16List,
                    },
                ]),
            },
            Field {
                path: &["laneAhead"],
                spec: Spec::StructList(&[
                    Field {
                        path: &["meta", "present"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["count"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["distanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["visible"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["available"],
                        spec: Spec::I16List,
                    },
                ]),
            },
            Field {
                path: &["speed"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["roadLimitValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["roadLimitKph"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["sdiPresent"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["sdiType"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["sdiDistanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["sdiSpeedLimitKph"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["sdiSectionType"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["sdiBlockType"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["sdiBlockSpeedKph"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["sdiBlockDistanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["secondarySdiPresent"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["secondarySdiType"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["secondarySdiDistanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["secondarySdiSpeedLimitKph"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["secondarySdiSectionType"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["secondarySdiBlockType"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["secondarySdiBlockSpeedKph"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["secondarySdiBlockDistanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["sectionPresent"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["sectionActive"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["sectionSpeedLimitKph"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["sectionAverageKph"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["sectionOverallAverageKph"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["sectionRemainingDistanceM"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["sectionRemainingTimeSec"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["sectionProgress"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["sectionSuspended"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["sectionOffRoute"],
                        spec: Spec::Bool,
                    },
                ]),
            },
            Field {
                path: &["trafficSignal"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["visible"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["distanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["redValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["redOn"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["redRemainSec"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["leftValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["leftOn"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["leftRemainSec"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["greenValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["greenOn"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["greenRemainSec"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["rightValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["rightOn"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["rightRemainSec"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["uturnValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["uturnOn"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["uturnRemainSec"],
                        spec: Spec::I16,
                    },
                    Field {
                        path: &["uiCounterValid"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["uiCounterRemainSec"],
                        spec: Spec::I16,
                    },
                ]),
            },
            Field {
                path: &["crossroad"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["visible"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["distanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["imageCode"],
                        spec: Spec::I32,
                    },
                ]),
            },
            Field {
                path: &["route"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["meta", "present"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["remainingDistanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["remainingTimeSec"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["movedDistanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["totalDistanceM"],
                        spec: Spec::I32,
                    },
                    Field {
                        path: &["polyline"],
                        spec: Spec::CoordList,
                    },
                ]),
            },
            Field {
                path: &["navigationStatus"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["guidanceActive"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["offRoute"],
                        spec: Spec::Bool,
                    },
                    Field {
                        path: &["routePresent"],
                        spec: Spec::Bool,
                    },
                ]),
            },
        ],
    ),
    (
        "livePose",
        20,
        &[
            Field {
                path: &["orientationNED"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["x"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["z"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["xStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["zStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["valid"],
                        spec: Spec::Bool,
                    },
                ]),
            },
            Field {
                path: &["velocityDevice"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["x"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["z"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["xStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["zStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["valid"],
                        spec: Spec::Bool,
                    },
                ]),
            },
            Field {
                path: &["accelerationDevice"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["x"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["z"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["xStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["zStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["valid"],
                        spec: Spec::Bool,
                    },
                ]),
            },
            Field {
                path: &["angularVelocityDevice"],
                spec: Spec::Struct(&[
                    Field {
                        path: &["x"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["y"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["z"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["xStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["yStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["zStd"],
                        spec: Spec::F32,
                    },
                    Field {
                        path: &["valid"],
                        spec: Spec::Bool,
                    },
                ]),
            },
            Field {
                path: &["inputsOK"],
                spec: Spec::Bool,
            },
            Field {
                path: &["posenetOK"],
                spec: Spec::Bool,
            },
            Field {
                path: &["sensorsOK"],
                spec: Spec::Bool,
            },
            Field {
                path: &["timestamp"],
                spec: Spec::U64,
            },
        ],
    ),
];
