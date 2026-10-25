define void @r_128_4_50_4(float* noalias align 32 %data0, i16* noalias align 32 %data1, i16* noalias align 32 %data2, i16* noalias align 32 %data3) #0
{
  %reg_0 = alloca [1 x float]
  %v0 = getelementptr inbounds float, float* %reg_0, i32 0
  br label %loop_entry_2_0
loop_entry_2_0:
  br label %loop_latch_2_0
loop_latch_2_0:
  %v1 = phi i32 [ 0, %loop_entry_2_0 ], [ %v1phi, %loop_footer_2_0 ]
  %v1phi = add i32 %v1, 1
  %v1cmp = icmp ult i32 %v1, 128
  br i1 %v1cmp, label %loop_body_2_0, label %loop_exit_2_0
loop_body_2_0:
  %v2 = mul i32 %v1, 6400
  %v3 = shl i32 %v1, 2
  br label %loop_entry_2_1
loop_entry_2_1:
  br label %loop_latch_2_1
loop_latch_2_1:
  %v4 = phi i32 [ 0, %loop_entry_2_1 ], [ %v4phi, %loop_footer_2_1 ]
  %v4phi = add i32 %v4, 1
  %v4cmp = icmp ult i32 %v4, 4
  br i1 %v4cmp, label %loop_body_2_1, label %loop_exit_2_1
loop_body_2_1:
  %v5 = getelementptr inbounds float, float* %reg_0, i32 0
  %v6 = shl i32 %v4, 2
  store float 0.0, float* %v5
  br label %loop_entry_0
loop_entry_0:
  br label %loop_latch_0
loop_latch_0:
  %v8 = phi i32 [ 0, %loop_entry_0 ], [ %v8phi, %loop_footer_0 ]
  %v8phi = add i32 %v8, 1
  %v8cmp = icmp ult i32 %v8, 50
  br i1 %v8cmp, label %loop_body_0, label %loop_exit_0
loop_body_0:
  %v9 = shl i32 %v8, 2
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
  %v21 = shl i32 %v8, 4
  %v22 = add nsw i32 %v21, %v6
  %v23 = add nsw i32 %v22, %v2
  %v24 = add nsw i32 %v23, 1
  %v25 = getelementptr inbounds i16, i16* %data2, i32 %v24
  %v26 = load i16, i16* %v25
  %v27 = add nsw i32 %v23, 2
  %v28 = getelementptr inbounds i16, i16* %data2, i32 %v27
  %v29 = load i16, i16* %v28
  %v30 = add nsw i32 %v23, 3
  %v31 = getelementptr inbounds i16, i16* %data2, i32 %v30
  %v32 = load i16, i16* %v31
  %v33 = getelementptr inbounds i16, i16* %data2, i32 %v23
  %v34 = load i16, i16* %v33
  %v35 = getelementptr inbounds float, float* %reg_0, i32 0
  %v36 = load float, float* %v35
  %v37 = and i16 %v12, 32767
  %v38 = zext i16 %v37 to i32
  %v39 = and i16 %v12, 32768
  %v40 = zext i16 %v39 to i32
  %v41 = and i16 %v15, 32767
  %v42 = zext i16 %v41 to i32
  %v43 = and i16 %v15, 32768
  %v44 = zext i16 %v43 to i32
  %v45 = and i16 %v18, 32767
  %v46 = zext i16 %v45 to i32
  %v47 = and i16 %v18, 32768
  %v48 = zext i16 %v47 to i32
  %v49 = and i16 %v20, 32767
  %v50 = zext i16 %v49 to i32
  %v51 = and i16 %v20, 32768
  %v52 = zext i16 %v51 to i32
  %v53 = shl i32 %v38, 13
  %v54 = add i32 %v53, 939524096
  %v55 = shl i32 %v40, 16
  %v56 = lshr i32 %v38, 10
  %v57 = icmp eq i32 %v56, 0
  %v58 = icmp eq i32 %v56, 31
  %v59 = or i32 %v53, 2139095040
  %v60 = select i1 %v58, i32 %v59, i32 %v54
  %v61 = select i1 %v57, i32 0, i32 %v60
  %v62 = or i32 %v55, %v61
  %v63 = bitcast i32 %v62 to float
  %v64 = shl i32 %v42, 13
  %v65 = add i32 %v64, 939524096
  %v66 = shl i32 %v44, 16
  %v67 = lshr i32 %v42, 10
  %v68 = icmp eq i32 %v67, 0
  %v69 = icmp eq i32 %v67, 31
  %v70 = or i32 %v64, 2139095040
  %v71 = select i1 %v69, i32 %v70, i32 %v65
  %v72 = select i1 %v68, i32 0, i32 %v71
  %v73 = or i32 %v66, %v72
  %v74 = bitcast i32 %v73 to float
  %v75 = shl i32 %v46, 13
  %v76 = add i32 %v75, 939524096
  %v77 = shl i32 %v48, 16
  %v78 = lshr i32 %v46, 10
  %v79 = icmp eq i32 %v78, 0
  %v80 = icmp eq i32 %v78, 31
  %v81 = or i32 %v75, 2139095040
  %v82 = select i1 %v80, i32 %v81, i32 %v76
  %v83 = select i1 %v79, i32 0, i32 %v82
  %v84 = or i32 %v77, %v83
  %v85 = bitcast i32 %v84 to float
  %v86 = shl i32 %v50, 13
  %v87 = add i32 %v86, 939524096
  %v88 = shl i32 %v52, 16
  %v89 = lshr i32 %v50, 10
  %v90 = icmp eq i32 %v89, 0
  %v91 = icmp eq i32 %v89, 31
  %v92 = or i32 %v86, 2139095040
  %v93 = select i1 %v91, i32 %v92, i32 %v87
  %v94 = select i1 %v90, i32 0, i32 %v93
  %v95 = or i32 %v88, %v94
  %v96 = bitcast i32 %v95 to float
  %v97_0 = insertelement <4 x float> poison, float %v96, i32 0
  %v97_1 = insertelement <4 x float> %v97_0, float %v63, i32 1
  %v97_2 = insertelement <4 x float> %v97_1, float %v74, i32 2
  %v97 = insertelement <4 x float> %v97_2, float %v85, i32 3
  %v98 = and i16 %v26, 32767
  %v99 = zext i16 %v98 to i32
  %v100 = and i16 %v26, 32768
  %v101 = zext i16 %v100 to i32
  %v102 = and i16 %v29, 32767
  %v103 = zext i16 %v102 to i32
  %v104 = and i16 %v29, 32768
  %v105 = zext i16 %v104 to i32
  %v106 = and i16 %v32, 32767
  %v107 = zext i16 %v106 to i32
  %v108 = and i16 %v32, 32768
  %v109 = zext i16 %v108 to i32
  %v110 = and i16 %v34, 32767
  %v111 = zext i16 %v110 to i32
  %v112 = and i16 %v34, 32768
  %v113 = zext i16 %v112 to i32
  %v114 = shl i32 %v99, 13
  %v115 = add i32 %v114, 939524096
  %v116 = shl i32 %v101, 16
  %v117 = lshr i32 %v99, 10
  %v118 = icmp eq i32 %v117, 0
  %v119 = icmp eq i32 %v117, 31
  %v120 = or i32 %v114, 2139095040
  %v121 = select i1 %v119, i32 %v120, i32 %v115
  %v122 = select i1 %v118, i32 0, i32 %v121
  %v123 = or i32 %v116, %v122
  %v124 = bitcast i32 %v123 to float
  %v125 = shl i32 %v103, 13
  %v126 = add i32 %v125, 939524096
  %v127 = shl i32 %v105, 16
  %v128 = lshr i32 %v103, 10
  %v129 = icmp eq i32 %v128, 0
  %v130 = icmp eq i32 %v128, 31
  %v131 = or i32 %v125, 2139095040
  %v132 = select i1 %v130, i32 %v131, i32 %v126
  %v133 = select i1 %v129, i32 0, i32 %v132
  %v134 = or i32 %v127, %v133
  %v135 = bitcast i32 %v134 to float
  %v136 = shl i32 %v107, 13
  %v137 = add i32 %v136, 939524096
  %v138 = shl i32 %v109, 16
  %v139 = lshr i32 %v107, 10
  %v140 = icmp eq i32 %v139, 0
  %v141 = icmp eq i32 %v139, 31
  %v142 = or i32 %v136, 2139095040
  %v143 = select i1 %v141, i32 %v142, i32 %v137
  %v144 = select i1 %v140, i32 0, i32 %v143
  %v145 = or i32 %v138, %v144
  %v146 = bitcast i32 %v145 to float
  %v147 = shl i32 %v111, 13
  %v148 = add i32 %v147, 939524096
  %v149 = shl i32 %v113, 16
  %v150 = lshr i32 %v111, 10
  %v151 = icmp eq i32 %v150, 0
  %v152 = icmp eq i32 %v150, 31
  %v153 = or i32 %v147, 2139095040
  %v154 = select i1 %v152, i32 %v153, i32 %v148
  %v155 = select i1 %v151, i32 0, i32 %v154
  %v156 = or i32 %v149, %v155
  %v157 = bitcast i32 %v156 to float
  %v158_0 = insertelement <4 x float> poison, float %v157, i32 0
  %v158_1 = insertelement <4 x float> %v158_0, float %v124, i32 1
  %v158_2 = insertelement <4 x float> %v158_1, float %v135, i32 2
  %v158 = insertelement <4 x float> %v158_2, float %v146, i32 3
  %v159 = extractelement <4 x float> %v97, i32 0
  %v160 = extractelement <4 x float> %v97, i32 1
  %v161 = extractelement <4 x float> %v97, i32 2
  %v162 = extractelement <4 x float> %v97, i32 3
  %v163 = extractelement <4 x float> %v158, i32 0
  %v164 = extractelement <4 x float> %v158, i32 1
  %v165 = extractelement <4 x float> %v158, i32 2
  %v166 = extractelement <4 x float> %v158, i32 3
  %v167 = fmul nsz arcp contract afn float %v159, %v163
  %v168 = fadd nsz arcp contract afn float %v36, %v167
  %v169 = fmul nsz arcp contract afn float %v160, %v164
  %v170 = fadd nsz arcp contract afn float %v168, %v169
  %v171 = fmul nsz arcp contract afn float %v161, %v165
  %v172 = fadd nsz arcp contract afn float %v170, %v171
  %v173 = fmul nsz arcp contract afn float %v162, %v166
  %v174 = fadd nsz arcp contract afn float %v172, %v173
  store float %v174, float* %v0
  br label %loop_footer_0
loop_footer_0:
  br label %loop_latch_0
loop_exit_0:
  %v177 = add nsw i32 %v3, %v4
  %v178 = getelementptr inbounds i16, i16* %data3, i32 %v177
  %v179 = load i16, i16* %v178
  %v180 = getelementptr inbounds float, float* %reg_0, i32 0
  %v181 = load float, float* %v180
  %v182 = getelementptr inbounds float, float* %data0, i32 %v177
  %v183 = and i16 %v179, 32767
  %v184 = zext i16 %v183 to i32
  %v185 = and i16 %v179, 32768
  %v186 = zext i16 %v185 to i32
  %v187 = shl i32 %v184, 13
  %v188 = add i32 %v187, 939524096
  %v189 = shl i32 %v186, 16
  %v190 = lshr i32 %v184, 10
  %v191 = icmp eq i32 %v190, 0
  %v192 = icmp eq i32 %v190, 31
  %v193 = or i32 %v187, 2139095040
  %v194 = select i1 %v192, i32 %v193, i32 %v188
  %v195 = select i1 %v191, i32 0, i32 %v194
  %v196 = or i32 %v189, %v195
  %v197 = bitcast i32 %v196 to float
  %v198 = fadd nsz arcp contract afn float %v181, %v197
  store float %v198, float* %v182
  br label %loop_footer_2_1
loop_footer_2_1:
  br label %loop_latch_2_1
loop_exit_2_1:
  br label %loop_footer_2_0
loop_footer_2_0:
  br label %loop_latch_2_0
loop_exit_2_0:
  ret void
}
attributes #0 = { alwaysinline nounwind "no-builtins" "no-trapping-math"="true" }