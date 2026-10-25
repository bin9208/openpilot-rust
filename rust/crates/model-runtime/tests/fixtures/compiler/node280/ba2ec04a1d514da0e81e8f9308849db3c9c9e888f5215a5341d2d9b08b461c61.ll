define void @E_128_50_4_4(i16* noalias align 32 %data0, i16* noalias align 32 %data1) #0
{
  br label %loop_entry_0
loop_entry_0:
  br label %loop_latch_0
loop_latch_0:
  %v0 = phi i32 [ 0, %loop_entry_0 ], [ %v0phi, %loop_footer_0 ]
  %v0phi = add i32 %v0, 1
  %v0cmp = icmp ult i32 %v0, 128
  br i1 %v0cmp, label %loop_body_0, label %loop_exit_0
loop_body_0:
  %v1 = mul i32 %v0, 800
  %v2 = mul i32 %v0, 6400
  br label %loop_entry_2
loop_entry_2:
  br label %loop_latch_2
loop_latch_2:
  %v3 = phi i32 [ 0, %loop_entry_2 ], [ %v3phi, %loop_footer_2 ]
  %v3phi = add i32 %v3, 1
  %v3cmp = icmp ult i32 %v3, 50
  br i1 %v3cmp, label %loop_body_2, label %loop_exit_2
loop_body_2:
  %v4 = shl i32 %v3, 2
  %v5 = shl i32 %v3, 4
  br label %loop_entry_3
loop_entry_3:
  br label %loop_latch_3
loop_latch_3:
  %v6 = phi i32 [ 0, %loop_entry_3 ], [ %v6phi, %loop_footer_3 ]
  %v6phi = add i32 %v6, 1
  %v6cmp = icmp ult i32 %v6, 4
  br i1 %v6cmp, label %loop_body_3, label %loop_exit_3
loop_body_3:
  %v7 = mul i32 %v6, 200
  %v8 = add nsw i32 %v1, %v7
  %v9 = add nsw i32 %v8, %v4
  %v10 = add nsw i32 %v9, 1
  %v11 = getelementptr inbounds i16, i16* %data1, i32 %v10
  %v12 = load i16, i16* %v11
  %v13 = add nsw i32 %v9, 2
  %v14 = getelementptr inbounds i16, i16* %data1, i32 %v13
  %v15 = load i16, i16* %v14
  %v16 = add nsw i32 %v9, 3
  %v17 = getelementptr inbounds i16, i16* %data1, i32 %v16
  %v18 = load i16, i16* %v17
  %v19 = getelementptr inbounds i16, i16* %data1, i32 %v9
  %v20 = load i16, i16* %v19
  %v21 = shl i32 %v6, 2
  %v22 = add nsw i32 %v5, %v21
  %v23 = add nsw i32 %v22, %v2
  %v24 = add nsw i32 %v23, 1
  %v25 = getelementptr inbounds i16, i16* %data0, i32 %v24
  %v26 = add nsw i32 %v23, 2
  %v27 = getelementptr inbounds i16, i16* %data0, i32 %v26
  %v28 = add nsw i32 %v23, 3
  %v29 = getelementptr inbounds i16, i16* %data0, i32 %v28
  %v30 = getelementptr inbounds i16, i16* %data0, i32 %v23
  %v31 = and i16 %v12, 32767
  %v32 = zext i16 %v31 to i32
  %v33 = and i16 %v12, 32768
  %v34 = zext i16 %v33 to i32
  %v35 = shl i32 %v32, 13
  %v36 = add i32 %v35, 939524096
  %v37 = shl i32 %v34, 16
  %v38 = lshr i32 %v32, 10
  %v39 = icmp eq i32 %v38, 0
  %v40 = icmp eq i32 %v38, 31
  %v41 = or i32 %v35, 2139095040
  %v42 = select i1 %v40, i32 %v41, i32 %v36
  %v43 = select i1 %v39, i32 0, i32 %v42
  %v44 = or i32 %v37, %v43
  %v45 = bitcast i32 %v44 to float
  %v46 = bitcast float %v45 to i32
  %v47 = bitcast i32 %v46 to float
  %v48 = fcmp nsz arcp contract afn olt float %v47, -65504.0
  %v49 = fcmp nsz arcp contract afn olt float 65504.0, %v47
  %v50 = fcmp nsz arcp contract afn une float %v47, %v47
  %v51 = select i1 %v49, float 0x7FF0000000000000, float %v47
  %v52 = select i1 %v48, float 0xFFF0000000000000, float %v51
  %v53 = select i1 %v50, float %v47, float %v52
  %v54 = bitcast float %v53 to i32
  %v55 = and i32 %v54, 2147483647
  %v56 = and i32 %v55, 4095
  %v57 = icmp ne i32 %v56, 0
  %v58 = zext i1 %v57 to i32
  %v59 = lshr i32 %v55, 12
  %v60 = lshr i32 %v55, 13
  %v61 = and i32 %v60, 1
  %v62 = or i32 %v58, %v61
  %v63 = and i32 %v59, 1
  %v64 = and i32 %v63, %v62
  %v65 = add i32 %v60, %v64
  %v66 = add i32 %v65, -114688
  %v67 = trunc i32 %v66 to i16
  %v68 = and i16 %v15, 32767
  %v69 = zext i16 %v68 to i32
  %v70 = and i16 %v15, 32768
  %v71 = zext i16 %v70 to i32
  %v72 = shl i32 %v69, 13
  %v73 = add i32 %v72, 939524096
  %v74 = shl i32 %v71, 16
  %v75 = lshr i32 %v69, 10
  %v76 = icmp eq i32 %v75, 0
  %v77 = icmp eq i32 %v75, 31
  %v78 = or i32 %v72, 2139095040
  %v79 = select i1 %v77, i32 %v78, i32 %v73
  %v80 = select i1 %v76, i32 0, i32 %v79
  %v81 = or i32 %v74, %v80
  %v82 = bitcast i32 %v81 to float
  %v83 = bitcast float %v82 to i32
  %v84 = bitcast i32 %v83 to float
  %v85 = fcmp nsz arcp contract afn olt float %v84, -65504.0
  %v86 = fcmp nsz arcp contract afn olt float 65504.0, %v84
  %v87 = fcmp nsz arcp contract afn une float %v84, %v84
  %v88 = select i1 %v86, float 0x7FF0000000000000, float %v84
  %v89 = select i1 %v85, float 0xFFF0000000000000, float %v88
  %v90 = select i1 %v87, float %v84, float %v89
  %v91 = bitcast float %v90 to i32
  %v92 = and i32 %v91, 2147483647
  %v93 = and i32 %v92, 4095
  %v94 = icmp ne i32 %v93, 0
  %v95 = zext i1 %v94 to i32
  %v96 = lshr i32 %v92, 12
  %v97 = lshr i32 %v92, 13
  %v98 = and i32 %v97, 1
  %v99 = or i32 %v95, %v98
  %v100 = and i32 %v96, 1
  %v101 = and i32 %v100, %v99
  %v102 = add i32 %v97, %v101
  %v103 = add i32 %v102, -114688
  %v104 = trunc i32 %v103 to i16
  %v105 = and i16 %v18, 32767
  %v106 = zext i16 %v105 to i32
  %v107 = and i16 %v18, 32768
  %v108 = zext i16 %v107 to i32
  %v109 = shl i32 %v106, 13
  %v110 = add i32 %v109, 939524096
  %v111 = shl i32 %v108, 16
  %v112 = lshr i32 %v106, 10
  %v113 = icmp eq i32 %v112, 0
  %v114 = icmp eq i32 %v112, 31
  %v115 = or i32 %v109, 2139095040
  %v116 = select i1 %v114, i32 %v115, i32 %v110
  %v117 = select i1 %v113, i32 0, i32 %v116
  %v118 = or i32 %v111, %v117
  %v119 = bitcast i32 %v118 to float
  %v120 = bitcast float %v119 to i32
  %v121 = bitcast i32 %v120 to float
  %v122 = fcmp nsz arcp contract afn olt float %v121, -65504.0
  %v123 = fcmp nsz arcp contract afn olt float 65504.0, %v121
  %v124 = fcmp nsz arcp contract afn une float %v121, %v121
  %v125 = select i1 %v123, float 0x7FF0000000000000, float %v121
  %v126 = select i1 %v122, float 0xFFF0000000000000, float %v125
  %v127 = select i1 %v124, float %v121, float %v126
  %v128 = bitcast float %v127 to i32
  %v129 = and i32 %v128, 2147483647
  %v130 = and i32 %v129, 4095
  %v131 = icmp ne i32 %v130, 0
  %v132 = zext i1 %v131 to i32
  %v133 = lshr i32 %v129, 12
  %v134 = lshr i32 %v129, 13
  %v135 = and i32 %v134, 1
  %v136 = or i32 %v132, %v135
  %v137 = and i32 %v133, 1
  %v138 = and i32 %v137, %v136
  %v139 = add i32 %v134, %v138
  %v140 = add i32 %v139, -114688
  %v141 = trunc i32 %v140 to i16
  %v142 = and i16 %v20, 32767
  %v143 = zext i16 %v142 to i32
  %v144 = and i16 %v20, 32768
  %v145 = zext i16 %v144 to i32
  %v146 = shl i32 %v143, 13
  %v147 = add i32 %v146, 939524096
  %v148 = shl i32 %v145, 16
  %v149 = lshr i32 %v143, 10
  %v150 = icmp eq i32 %v149, 0
  %v151 = icmp eq i32 %v149, 31
  %v152 = or i32 %v146, 2139095040
  %v153 = select i1 %v151, i32 %v152, i32 %v147
  %v154 = select i1 %v150, i32 0, i32 %v153
  %v155 = or i32 %v148, %v154
  %v156 = bitcast i32 %v155 to float
  %v157 = bitcast float %v156 to i32
  %v158 = bitcast i32 %v157 to float
  %v159 = fcmp nsz arcp contract afn olt float %v158, -65504.0
  %v160 = fcmp nsz arcp contract afn olt float 65504.0, %v158
  %v161 = fcmp nsz arcp contract afn une float %v158, %v158
  %v162 = select i1 %v160, float 0x7FF0000000000000, float %v158
  %v163 = select i1 %v159, float 0xFFF0000000000000, float %v162
  %v164 = select i1 %v161, float %v158, float %v163
  %v165 = bitcast float %v164 to i32
  %v166 = and i32 %v165, 2147483647
  %v167 = and i32 %v166, 4095
  %v168 = icmp ne i32 %v167, 0
  %v169 = zext i1 %v168 to i32
  %v170 = lshr i32 %v166, 12
  %v171 = lshr i32 %v166, 13
  %v172 = and i32 %v171, 1
  %v173 = or i32 %v169, %v172
  %v174 = and i32 %v170, 1
  %v175 = and i32 %v174, %v173
  %v176 = add i32 %v171, %v175
  %v177 = add i32 %v176, -114688
  %v178 = trunc i32 %v177 to i16
  %v179 = lshr i32 %v54, 16
  %v180 = and i32 %v179, 32768
  %v181 = and i32 %v60, 1023
  %v182 = or i32 %v180, %v181
  %v183 = or i32 %v182, 31744
  %v184 = trunc i32 %v183 to i16
  %v185 = lshr i32 %v91, 16
  %v186 = and i32 %v185, 32768
  %v187 = and i32 %v97, 1023
  %v188 = or i32 %v186, %v187
  %v189 = or i32 %v188, 31744
  %v190 = trunc i32 %v189 to i16
  %v191 = lshr i32 %v128, 16
  %v192 = and i32 %v191, 32768
  %v193 = and i32 %v134, 1023
  %v194 = or i32 %v192, %v193
  %v195 = or i32 %v194, 31744
  %v196 = trunc i32 %v195 to i16
  %v197 = lshr i32 %v165, 16
  %v198 = and i32 %v197, 32768
  %v199 = and i32 %v171, 1023
  %v200 = or i32 %v198, %v199
  %v201 = or i32 %v200, 31744
  %v202 = trunc i32 %v201 to i16
  %v203 = trunc i32 %v180 to i16
  %v204 = trunc i32 %v186 to i16
  %v205 = trunc i32 %v192 to i16
  %v206 = trunc i32 %v198 to i16
  %v207 = lshr i32 %v54, 23
  %v208 = lshr i32 %v91, 23
  %v209 = lshr i32 %v128, 23
  %v210 = lshr i32 %v165, 23
  %v211 = and i32 %v207, 255
  %v212 = icmp ult i32 %v211, 113
  %v213 = and i32 %v208, 255
  %v214 = icmp ult i32 %v213, 113
  %v215 = and i32 %v209, 255
  %v216 = icmp ult i32 %v215, 113
  %v217 = and i32 %v210, 255
  %v218 = icmp ult i32 %v217, 113
  %v219 = icmp eq i32 %v211, 255
  %v220 = icmp eq i32 %v213, 255
  %v221 = icmp eq i32 %v215, 255
  %v222 = icmp eq i32 %v217, 255
  %v223 = select i1 %v212, i16 0, i16 %v67
  %v224 = or i16 %v203, %v223
  %v225 = select i1 %v214, i16 0, i16 %v104
  %v226 = or i16 %v204, %v225
  %v227 = select i1 %v216, i16 0, i16 %v141
  %v228 = or i16 %v205, %v227
  %v229 = select i1 %v218, i16 0, i16 %v178
  %v230 = or i16 %v206, %v229
  %v231 = select i1 %v219, i16 %v184, i16 %v224
  %v232 = select i1 %v220, i16 %v190, i16 %v226
  %v233 = select i1 %v221, i16 %v196, i16 %v228
  %v234 = select i1 %v222, i16 %v202, i16 %v230
  store i16 %v231, i16* %v25
  store i16 %v232, i16* %v27
  store i16 %v233, i16* %v29
  store i16 %v234, i16* %v30
  br label %loop_footer_3
loop_footer_3:
  br label %loop_latch_3
loop_exit_3:
  br label %loop_footer_2
loop_footer_2:
  br label %loop_latch_2
loop_exit_2:
  br label %loop_footer_0
loop_footer_0:
  br label %loop_latch_0
loop_exit_0:
  ret void
}
attributes #0 = { alwaysinline nounwind "no-builtins" "no-trapping-math"="true" }